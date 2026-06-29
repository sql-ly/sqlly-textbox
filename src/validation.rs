//! Validation state, sync validator, and async debounced validator runner.

use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidationState {
    /// No validation has been performed or it's irrelevant for this field.
    None,
    /// A debounced async validation is in flight.
    Validating,
    /// Value is valid.
    Valid,
    /// Value is acceptable but warrants a soft warning to the user.
    Warning(String),
    /// Value is invalid; the message explains why.
    Invalid(String),
}

impl ValidationState {
    pub fn is_known(&self) -> bool {
        !matches!(self, ValidationState::None | ValidationState::Validating)
    }

    pub fn is_invalid(&self) -> bool {
        matches!(self, ValidationState::Invalid(_))
    }

    pub fn is_warning(&self) -> bool {
        matches!(self, ValidationState::Warning(_))
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            ValidationState::Invalid(m) | ValidationState::Warning(m) => Some(m.as_str()),
            _ => None,
        }
    }
}

impl Default for ValidationState {
    fn default() -> Self {
        ValidationState::None
    }
}

/// A synchronous validator: maps a value to a `ValidationState`.
pub type SyncValidator = Arc<dyn Fn(&str) -> ValidationState + Send + Sync>;

/// Construct a `SyncValidator` from a closure.
pub fn sync_validator<F>(f: F) -> SyncValidator
where
    F: Fn(&str) -> ValidationState + Send + Sync + 'static,
{
    Arc::new(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_none() {
        assert_eq!(ValidationState::default(), ValidationState::None);
        assert!(!ValidationState::default().is_known());
    }

    #[test]
    fn helper_classifiers() {
        assert!(ValidationState::Invalid("nope".into()).is_invalid());
        assert!(ValidationState::Warning("todo".into()).is_warning());
        assert!(!ValidationState::Valid.is_invalid());
        assert!(!ValidationState::Validating.is_warning());
        assert_eq!(ValidationState::Invalid("x".into()).message(), Some("x"));
        assert_eq!(ValidationState::Valid.message(), None);
    }

    #[test]
    fn sync_validator_from_closure() {
        let v = sync_validator(|s: &str| {
            if s.contains('@') {
                ValidationState::Valid
            } else {
                ValidationState::Invalid("must contain @".into())
            }
        });
        assert_eq!(v("a@b"), ValidationState::Valid);
        assert!(matches!(v("nope"), ValidationState::Invalid(_)));
    }
}
