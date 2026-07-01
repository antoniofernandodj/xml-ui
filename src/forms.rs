//! Angular-Reactive-Forms-inspired form state: [`FormBuilder`] declares a
//! [`Form`]'s [`FormControl`]s (name, initial value, validators), the
//! component owns the built `Form` in its own state, and the `<Form>`/
//! `formControl` template attributes (see `parser.rs`/`eval.rs`) bind inputs
//! to it by name — mirroring Angular's `FormGroup`/`formControlName`.
//!
//! ```
//! use glacier_ui::{FormBuilder, FormControl};
//!
//! let mut form = FormBuilder::new("login")
//!     .control(FormControl::new("username", "").required().min_length(3))
//!     .control(FormControl::new("password", "").required().min_length(6))
//!     .build();
//!
//! assert!(!form.is_valid());
//! form.set_value("username", "ana");
//! form.set_value("password", "hunter2");
//! assert!(form.is_valid());
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use crate::component::Context;

/// A single validation rule for a [`FormControl`]'s current value.
#[derive(Clone)]
pub enum Validator {
    /// The value must not be empty (after trimming whitespace).
    Required,
    /// The value must have at least this many characters.
    MinLength(usize),
    /// The value must have at most this many characters.
    MaxLength(usize),
    /// The value must match this regular expression (a value only fails if
    /// the pattern is well-formed and does *not* match — a malformed pattern
    /// is reported as its own error instead of silently passing).
    Pattern(String),
    /// Any other rule: `Ok(())` when valid, `Err(message)` otherwise.
    Custom(Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>),
}

impl std::fmt::Debug for Validator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Validator::Required => write!(f, "Required"),
            Validator::MinLength(n) => write!(f, "MinLength({n})"),
            Validator::MaxLength(n) => write!(f, "MaxLength({n})"),
            Validator::Pattern(p) => write!(f, "Pattern({p:?})"),
            Validator::Custom(_) => write!(f, "Custom(..)"),
        }
    }
}

impl Validator {
    /// Checks `value` against this rule, returning a user-facing error
    /// message naming `field` when it fails.
    fn check(&self, field: &str, value: &str) -> Result<(), String> {
        match self {
            Validator::Required => {
                if value.trim().is_empty() {
                    Err(format!("\"{field}\" is required"))
                } else {
                    Ok(())
                }
            }
            Validator::MinLength(n) => {
                if value.chars().count() < *n {
                    Err(format!("\"{field}\" must be at least {n} characters"))
                } else {
                    Ok(())
                }
            }
            Validator::MaxLength(n) => {
                if value.chars().count() > *n {
                    Err(format!("\"{field}\" must be at most {n} characters"))
                } else {
                    Ok(())
                }
            }
            Validator::Pattern(pattern) => match regex::Regex::new(pattern) {
                Ok(re) => {
                    if re.is_match(value) {
                        Ok(())
                    } else {
                        Err(format!("\"{field}\" does not match the expected format"))
                    }
                }
                Err(e) => Err(format!("\"{field}\" has an invalid pattern: {e}")),
            },
            Validator::Custom(f) => f(value),
        }
    }
}

/// A single field of a [`Form`]: a name, a current value, and the validators
/// that value must satisfy. Built via [`FormControl::new`] and the builder
/// methods ([`FormControl::required`], etc.), then added to a [`FormBuilder`].
#[derive(Clone, Debug)]
pub struct FormControl {
    name: String,
    value: String,
    initial_value: String,
    validators: Vec<Validator>,
    touched: bool,
    errors: Vec<String>,
}

impl FormControl {
    /// A new control named `name`, seeded with `initial_value` and no
    /// validators (add them with `.required()`/`.min_length()`/etc.).
    pub fn new(name: impl Into<String>, initial_value: impl Into<String>) -> Self {
        let value = initial_value.into();
        Self {
            name: name.into(),
            value: value.clone(),
            initial_value: value,
            validators: Vec::new(),
            touched: false,
            errors: Vec::new(),
        }
    }

    /// The value must not be empty (after trimming whitespace).
    pub fn required(mut self) -> Self {
        self.validators.push(Validator::Required);
        self
    }

    /// The value must have at least `n` characters.
    pub fn min_length(mut self, n: usize) -> Self {
        self.validators.push(Validator::MinLength(n));
        self
    }

    /// The value must have at most `n` characters.
    pub fn max_length(mut self, n: usize) -> Self {
        self.validators.push(Validator::MaxLength(n));
        self
    }

    /// The value must match the regular expression `pattern`.
    pub fn pattern(mut self, pattern: impl Into<String>) -> Self {
        self.validators.push(Validator::Pattern(pattern.into()));
        self
    }

    /// Any other rule: `f` returns `Ok(())` when `value` is valid, or
    /// `Err(message)` with a user-facing explanation otherwise.
    pub fn validator<F>(mut self, f: F) -> Self
    where
        F: Fn(&str) -> Result<(), String> + Send + Sync + 'static,
    {
        self.validators.push(Validator::Custom(Arc::new(f)));
        self
    }

    /// This control's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// This control's current value.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Whether the value has ever been changed via [`FormControl::set_value`]
    /// (or the control force-validated via [`Form::validate`]).
    pub fn touched(&self) -> bool {
        self.touched
    }

    /// The errors from the last validation, in validator declaration order.
    /// Populated by [`FormControl::set_value`] and [`Form::validate`]; use
    /// [`FormControl::is_valid`] for a check that is always fresh.
    pub fn errors(&self) -> &[String] {
        &self.errors
    }

    /// Updates the value, marks the control touched, and re-runs its
    /// validators (see [`FormControl::errors`]).
    pub fn set_value(&mut self, value: impl Into<String>) {
        self.value = value.into();
        self.touched = true;
        self.errors = self.collect_errors();
    }

    /// Restores the initial value and clears touched/errors.
    pub fn reset(&mut self) {
        self.value = self.initial_value.clone();
        self.touched = false;
        self.errors.clear();
    }

    /// Runs every validator against the current value fresh (independent of
    /// the cached [`FormControl::errors`]) and reports whether all passed.
    pub fn is_valid(&self) -> bool {
        self.collect_errors().is_empty()
    }

    fn collect_errors(&self) -> Vec<String> {
        self.validators
            .iter()
            .filter_map(|v| v.check(&self.name, &self.value).err())
            .collect()
    }
}

/// Declares a [`Form`]'s controls, in the order inputs should be visited when
/// the user presses Enter to move to the next field.
///
/// ```
/// use glacier_ui::{FormBuilder, FormControl};
///
/// let form = FormBuilder::new("signup")
///     .control(FormControl::new("email", "").required().pattern(r"^[^@\s]+@[^@\s]+\.[^@\s]+$"))
///     .control(FormControl::new("password", "").required().min_length(6))
///     .build();
/// assert_eq!(form.control_names().collect::<Vec<_>>(), vec!["email", "password"]);
/// ```
pub struct FormBuilder {
    name: String,
    controls: Vec<FormControl>,
}

impl FormBuilder {
    /// Starts building a form named `name` (matched against a `<Form
    /// name="...">`'s `name` attribute when more than one form shares a
    /// component; otherwise it can be any label).
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), controls: Vec::new() }
    }

    /// Registers a control, in the order controls should be visited by Enter.
    pub fn control(mut self, control: FormControl) -> Self {
        self.controls.push(control);
        self
    }

    /// Finishes building the [`Form`].
    pub fn build(self) -> Form {
        Form { name: self.name, controls: self.controls }
    }
}

/// A group of [`FormControl`]s built by [`FormBuilder`]. Owned by the
/// component's own state (not the engine), synced into the reactive
/// [`Context`] via [`Form::sync_to_context`] so `formControl`-bound inputs
/// read/write it.
pub struct Form {
    name: String,
    controls: Vec<FormControl>,
}

impl Form {
    /// This form's name (see [`FormBuilder::new`]).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The control named `name`, if any.
    pub fn get(&self, name: &str) -> Option<&FormControl> {
        self.controls.iter().find(|c| c.name == name)
    }

    /// A mutable reference to the control named `name`, if any.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut FormControl> {
        self.controls.iter_mut().find(|c| c.name == name)
    }

    /// Whether a control named `name` is registered on this form. Handy in
    /// `Component::update` to dispatch a form-control action generically:
    /// ```ignore
    /// if self.form.has_control(action) {
    ///     self.form.set_value(action, value.unwrap_or_default());
    ///     self.form.sync_to_context(ctx);
    /// }
    /// ```
    pub fn has_control(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// The current value of `name` (`""` if there's no such control).
    pub fn value(&self, name: &str) -> &str {
        self.get(name).map(FormControl::value).unwrap_or("")
    }

    /// Updates the value of the control named `name`, if it exists (a no-op
    /// otherwise).
    pub fn set_value(&mut self, name: &str, value: impl Into<String>) {
        if let Some(c) = self.get_mut(name) {
            c.set_value(value);
        }
    }

    /// The cached errors of `name` (`&[]` if there's no such control, or it
    /// hasn't been validated yet).
    pub fn errors(&self, name: &str) -> &[String] {
        self.get(name).map(FormControl::errors).unwrap_or(&[])
    }

    /// Whether every control currently passes its validators. Always fresh —
    /// safe to call before the user has touched anything (e.g. a submit
    /// button that starts disabled).
    pub fn is_valid(&self) -> bool {
        self.controls.iter().all(FormControl::is_valid)
    }

    /// Force-runs every control's validators, marking all of them touched and
    /// caching the resulting errors — so a submit handler can surface errors
    /// on fields the user never edited. Returns the same as
    /// [`Form::is_valid`].
    pub fn validate(&mut self) -> bool {
        let mut all_valid = true;
        for c in self.controls.iter_mut() {
            c.touched = true;
            c.errors = c.collect_errors();
            if !c.errors.is_empty() {
                all_valid = false;
            }
        }
        all_valid
    }

    /// Restores every control to its initial value and clears touched/errors.
    pub fn reset(&mut self) {
        for c in self.controls.iter_mut() {
            c.reset();
        }
    }

    /// Every control's name, in declaration order.
    pub fn control_names(&self) -> impl Iterator<Item = &str> {
        self.controls.iter().map(FormControl::name)
    }

    /// A snapshot of every control's current value, keyed by name.
    pub fn values(&self) -> HashMap<String, String> {
        self.controls.iter().map(|c| (c.name.clone(), c.value.clone())).collect()
    }

    /// Publishes every control's current value into the reactive context under
    /// its own name, so `formControl`-bound inputs (and any `{name}`
    /// placeholder) reflect the form's state. Call this from
    /// `Component::init`/`update` after any change made directly through the
    /// `Form` API (a change coming from the input itself round-trips through
    /// `Form::set_value` already).
    pub fn sync_to_context(&self, ctx: &mut Context) {
        for c in &self.controls {
            ctx.set(&c.name, c.value.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_control_is_invalid_until_set() {
        let form = FormBuilder::new("f")
            .control(FormControl::new("username", ""))
            .build();
        // No validators yet: an empty value is still "valid".
        assert!(form.is_valid());

        let form = FormBuilder::new("f")
            .control(FormControl::new("username", "").required())
            .build();
        assert!(!form.is_valid());
    }

    #[test]
    fn set_value_updates_touched_and_errors_live() {
        let mut form = FormBuilder::new("f")
            .control(FormControl::new("username", "").required().min_length(3))
            .build();

        assert!(!form.get("username").unwrap().touched());
        assert!(!form.is_valid());

        form.set_value("username", "ab");
        assert!(form.get("username").unwrap().touched());
        assert!(!form.is_valid());
        assert_eq!(form.errors("username").len(), 1);

        form.set_value("username", "abc");
        assert!(form.is_valid());
        assert!(form.errors("username").is_empty());
    }

    #[test]
    fn min_and_max_length() {
        let mut form = FormBuilder::new("f")
            .control(FormControl::new("bio", "").min_length(2).max_length(4))
            .build();

        form.set_value("bio", "a");
        assert!(!form.is_valid());
        form.set_value("bio", "ab");
        assert!(form.is_valid());
        form.set_value("bio", "abcd");
        assert!(form.is_valid());
        form.set_value("bio", "abcde");
        assert!(!form.is_valid());
    }

    #[test]
    fn pattern_validator() {
        let mut form = FormBuilder::new("f")
            .control(FormControl::new("email", "").pattern(r"^[^@\s]+@[^@\s]+\.[^@\s]+$"))
            .build();

        form.set_value("email", "not-an-email");
        assert!(!form.is_valid());
        form.set_value("email", "user@example.com");
        assert!(form.is_valid());
    }

    #[test]
    fn custom_validator_closure() {
        let mut form = FormBuilder::new("f")
            .control(
                FormControl::new("age", "0").validator(|v| {
                    v.parse::<u32>()
                        .ok()
                        .filter(|n| *n >= 18)
                        .map(|_| ())
                        .ok_or_else(|| "must be an adult".to_string())
                }),
            )
            .build();

        assert!(!form.is_valid());
        form.set_value("age", "17");
        assert!(!form.is_valid());
        assert_eq!(form.errors("age"), &["must be an adult".to_string()]);
        form.set_value("age", "18");
        assert!(form.is_valid());
    }

    #[test]
    fn validate_marks_every_control_touched_even_if_untouched() {
        let mut form = FormBuilder::new("f")
            .control(FormControl::new("a", "").required())
            .control(FormControl::new("b", "ok"))
            .build();

        assert!(!form.get("a").unwrap().touched());
        let ok = form.validate();
        assert!(!ok);
        assert!(form.get("a").unwrap().touched());
        assert!(form.get("b").unwrap().touched());
        assert_eq!(form.errors("a").len(), 1);
    }

    #[test]
    fn reset_restores_initial_value() {
        let mut form = FormBuilder::new("f")
            .control(FormControl::new("name", "Ana"))
            .build();

        form.set_value("name", "Bob");
        assert_eq!(form.value("name"), "Bob");
        form.reset();
        assert_eq!(form.value("name"), "Ana");
        assert!(!form.get("name").unwrap().touched());
    }

    #[test]
    fn values_and_control_names_preserve_declaration_order() {
        let form = FormBuilder::new("f")
            .control(FormControl::new("first", "1"))
            .control(FormControl::new("second", "2"))
            .build();

        assert_eq!(form.control_names().collect::<Vec<_>>(), vec!["first", "second"]);
        let values = form.values();
        assert_eq!(values.get("first").map(String::as_str), Some("1"));
        assert_eq!(values.get("second").map(String::as_str), Some("2"));
    }

    #[test]
    fn has_control_and_missing_control_are_harmless() {
        let mut form = FormBuilder::new("f")
            .control(FormControl::new("x", ""))
            .build();

        assert!(form.has_control("x"));
        assert!(!form.has_control("y"));
        // Setting a value on a control that doesn't exist is a no-op, not a panic.
        form.set_value("y", "z");
        assert_eq!(form.value("y"), "");
        assert!(form.errors("y").is_empty());
    }
}
