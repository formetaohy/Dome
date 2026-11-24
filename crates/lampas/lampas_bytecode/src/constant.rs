use dome_math::Vec3;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub enum Constant {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(Arc<str>),
    Vector(Vec3),
    List(Vec<Constant>),
}
