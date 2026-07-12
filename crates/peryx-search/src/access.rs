//! Search carries ACL predicates into the query so totals and pagination cannot leak private resources.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchAccess {
    pub(crate) patterns: Vec<SearchAccessPattern>,
}

impl SearchAccess {
    #[must_use]
    pub const fn new(patterns: Vec<SearchAccessPattern>) -> Self {
        Self { patterns }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SearchAccessPattern {
    pub route: String,
    pub glob: String,
}
