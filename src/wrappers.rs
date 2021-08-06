use regex::Regex;
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug)]
pub struct RegexEq(pub Regex);

impl Hash for RegexEq {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_str().hash(state);
    }
}

impl PartialEq for RegexEq {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str().eq(other.0.as_str())
    }
}
impl Eq for RegexEq {}

impl std::ops::Deref for RegexEq {
    type Target = Regex;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub struct Float(pub f64);

impl Hash for Float {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

impl PartialEq for Float {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits().eq(&other.0.to_bits())
    }
}
impl Eq for Float {}

impl std::ops::Deref for Float {
    type Target = f64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<f64> for Float {
    fn from(num: f64) -> Self {
        Float(num)
    }
}

impl From<Float> for f64 {
    fn from(num: Float) -> Self {
        num.0
    }
}
