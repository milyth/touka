use std::{collections::HashMap, error::Error, fs::File};

use std::io::Write;

type GenericResult<T> = Result<T, Box<dyn Error + Sync + Send>>;
use crate::ast::{Binary, File as AstRoot, Term};

const STR: u8 = 0xca;
const INT: u8 = 0xfe;
const MAYBE: u8 = 0xba;

#[derive(Default)]
pub struct State {
    constants: HashMap<usize, (String, String)>,
    types: HashMap<usize, u8>,
    print_queue: Vec<usize>,
    runtime_queue: HashMap<usize, String>,
    it: usize,
}

impl State {
    fn bag_or_die(self: &mut Self, term: Term) -> Term {
        match term {
            Term::Print(p) => {
                let id = self.inspect(&p.value);
                self.print_queue.push(id);
                self.it += 1;

                *p.value
            }

            _ => {
                self.inspect(&term);

                term
            }
        }
    }

    fn inspect(self: &mut Self, term: &Term) -> usize {
        self.it += 1;

        macro_rules! int {
            ($to:expr, $value:expr) => {{
                self.constants
                    .insert($to, ("int".to_string(), format!("{}", $value)));
                self.types.insert($to, INT);
            }};
        }

        macro_rules! maybe {
            ($to:expr, $value:expr) => {{
                self.constants
                    .insert($to, ("char".to_string(), format!("{}", $value)));
                self.types.insert($to, MAYBE);
            }};
        }

        macro_rules! loveint {
            ($it:expr, $binary:ident, $nm: expr, $op:tt) => {
                match (self.bag_or_die(*$binary.lhs.clone()), self.bag_or_die(*$binary.rhs.clone())) {
                    (Term::Int(x), Term::Int(z)) => {
                        int!($it, x.value $op z.value);
                    }

                    what => panic!("{} => Just ints. found {what:?}", $nm),
                }
            };
        }

        macro_rules! loveintcomp {
            ($it:expr, $binary:ident, $nm: expr, $op:tt) => {
                match (self.bag_or_die(*$binary.lhs.clone()), self.bag_or_die(*$binary.rhs.clone())) {
                    (Term::Int(x), Term::Int(z)) => {
                        maybe!($it, x.value $op z.value);
                    }

                    _ => panic!(concat!($nm, "=> Just ints.")),
                }
            };
        }

        macro_rules! phonk {
            ($it:expr, $value:expr) => {{
                self.constants.insert($it, ("char*".to_string(), $value));
                self.types.insert($it, STR);
            }};
        }

        match term {
            Term::Str(s) => {
                phonk!(self.it, format!("{:?}", s.value));
            }
            Term::Int(i) => {
                int!(self.it, i.value);
            }

            Term::If(comp) => match self.bag_or_die(*comp.condition.clone()) {
                Term::Bool(b) => {
                    let res = if b.value {
                        self.inspect(&comp.then)
                    } else {
                        self.inspect(&comp.otherwise)
                    };

                    panic!("{}", res);
                }
                t @ Term::Binary(_) => {
                    let res = self.inspect(&t);
                    if self.constants.get(&res).unwrap().1 == "true" {
                        self.inspect(&comp.then)
                    } else {
                        self.inspect(&comp.otherwise)
                    };
                }
                what => panic!("If => Just boolean or binary. found {what:?}"),
            },

            Term::Binary(binary) => match binary.op {
                crate::ast::BinaryOp::Add => match (
                    self.bag_or_die(*binary.lhs.clone()),
                    self.bag_or_die(*binary.rhs.clone()),
                ) {
                    (Term::Int(x), Term::Int(z)) => {
                        int!(self.it, x.value + z.value);
                    }

                    (Term::Str(s), Term::Str(s2)) => {
                        phonk!(self.it, format!("{:?}", s.value + &s2.value));
                    }

                    what => panic!("Add => Just ints and strings. found {what:?}"),
                },

                crate::ast::BinaryOp::Div => loveint!(self.it, binary, "Div", %),
                crate::ast::BinaryOp::Sub => loveint!(self.it, binary, "Sub", *),
                crate::ast::BinaryOp::Rem => loveint!(self.it, binary, "Rem", %),
                crate::ast::BinaryOp::Mul => loveint!(self.it, binary, "Mul", *),
                crate::ast::BinaryOp::Lt => loveintcomp!(self.it, binary, "Lt", <),
                crate::ast::BinaryOp::Gt => loveintcomp!(self.it, binary, "Gt", >),
                crate::ast::BinaryOp::Lte => loveintcomp!(self.it, binary, "Lte", >=),
                crate::ast::BinaryOp::Gte => loveintcomp!(self.it, binary, "Gte", <=),

                crate::ast::BinaryOp::Eq => match (*binary.lhs.clone(), *binary.rhs.clone()) {
                    (Term::Int(x), Term::Int(z)) => {
                        maybe!(self.it, x.value == z.value);
                    }

                    (Term::Str(s), Term::Str(s2)) => {
                        maybe!(self.it, s.value == s2.value);
                        // self.runtime_queue
                        //     .insert(self.it, format!("!strcmp({:?}, {:?})", s.value, s2.value));
                    }

                    (Term::Bool(b), Term::Bool(b2)) => {
                        maybe!(self.it, b.value == b2.value);
                    }

                    _ => panic!("Eq => Invalid types!"),
                },

                crate::ast::BinaryOp::Neq => match (*binary.lhs.clone(), *binary.rhs.clone()) {
                    (Term::Int(x), Term::Int(z)) => {
                        maybe!(self.it, x.value != z.value);
                    }

                    (Term::Str(s), Term::Str(s2)) => {
                        maybe!(self.it, s.value != s2.value);
                        // self.runtime_queue
                        //     .insert(self.it, format!("!!strcmp({:?}, {:?})", s.value, s2.value));
                    }

                    (Term::Bool(b), Term::Bool(b2)) => {
                        maybe!(self.it, b.value != b2.value);
                    }

                    _ => todo!(),
                },

                crate::ast::BinaryOp::And => match (*binary.lhs.clone(), *binary.rhs.clone()) {
                    (Term::Bool(b), Term::Bool(b2)) => {
                        maybe!(self.it, b.value && b2.value);
                    }

                    _ => panic!("Just bools are allowed."),
                },

                crate::ast::BinaryOp::Or => match (*binary.lhs.clone(), *binary.rhs.clone()) {
                    (Term::Bool(b), Term::Bool(b2)) => {
                        maybe!(self.it, b.value || b2.value);
                    }

                    _ => panic!("Just bools are allowed."),
                },
            },

            _ => {}
        }
        return self.it;
    }

    pub fn write(self: Self) -> GenericResult<()> {
        let mut output = File::create("output.c")?;

        writeln!(output, "{}", include_str!("yamero.c"))?;

        for (j, (k, v)) in self.constants {
            writeln!(output, "{} v_{} = {};", k, j, v)?
        }

        for (j, k) in self.types {
            writeln!(output, "const Kind t_{j} = {k};")?;
        }

        writeln!(output, "int main(void) {{")?;

        for (id, expr) in self.runtime_queue {
            writeln!(output, "v_{id} = {expr};")?;
        }

        for item in self.print_queue {
            writeln!(output, "p((void*)&v_{item}, t_{item});")?;
        }

        writeln!(output, "return 0;}}")?;

        Ok(())
    }

    pub fn generate(self: &mut Self, source: AstRoot) -> GenericResult<()> {
        match source.expression {
            crate::ast::Term::Error(_) => todo!(),
            crate::ast::Term::Int(_) => todo!(),
            crate::ast::Term::Str(_) => todo!(),
            crate::ast::Term::Call(_) => todo!(),
            crate::ast::Term::Binary(_) => todo!(),
            crate::ast::Term::Function(_) => todo!(),
            crate::ast::Term::Let(_) => todo!(),
            crate::ast::Term::If(_) => todo!(),
            crate::ast::Term::Print(what) => {
                let it = self.inspect(&what.value);

                self.print_queue.push(it);
            }
            crate::ast::Term::First(_) => todo!(),
            crate::ast::Term::Second(_) => todo!(),
            crate::ast::Term::Bool(_) => todo!(),
            crate::ast::Term::Tuple(_) => todo!(),
            crate::ast::Term::Var(_) => todo!(),
        }

        Ok(())
    }
}
