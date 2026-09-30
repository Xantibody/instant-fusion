//! Schemes shared by the tests.

pub const DAYFOX: &str = include_str!("../tests/fixtures/dayfox.yaml");

pub fn dayfox() -> crate::scheme::Scheme {
    crate::scheme::Scheme::parse(DAYFOX).unwrap()
}

/// dayfox with a dark base00
pub fn dark() -> crate::scheme::Scheme {
    crate::scheme::Scheme::parse(&DAYFOX.replace("f6f2ee", "1d1a26")).unwrap()
}
