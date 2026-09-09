/// Logging subscriber setup.
pub mod logging;

use std::fmt::Display;

/// Ignore an error while keeping it visible in the logs.
///
/// The project forbids silently swallowing fallible results with bare
/// `let _ = fallible()`. When an error is genuinely non-fatal (a dropped
/// progress receiver, a logger that is already initialised, …) call
/// [`LogErr::log_err`] instead so the failure is surfaced at `warn` level,
/// tagged with the call site, rather than discarded.
pub trait LogErr<T> {
    /// Return `Some(value)` on success. On error, log it via `tracing::warn!`
    /// (tagged with the caller's location) and return `None`.
    fn log_err(self) -> Option<T>;
}

impl<T, E: Display> LogErr<T> for Result<T, E> {
    #[track_caller]
    fn log_err(self) -> Option<T> {
        match self {
            Ok(value) => Some(value),
            Err(error) => {
                let location = std::panic::Location::caller();
                tracing::warn!("{}:{}: {error}", location.file(), location.line());
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LogErr;

    #[test]
    fn ok_is_passed_through() {
        let result: Result<u8, &str> = Ok(7);
        assert_eq!(result.log_err(), Some(7));
    }

    #[test]
    fn err_is_logged_and_discarded() {
        let result: Result<u8, &str> = Err("boom");
        assert_eq!(result.log_err(), None);
    }

    #[test]
    fn works_across_value_and_error_types() {
        let owned: Result<String, String> = Ok("kept".to_string());
        assert_eq!(owned.log_err().as_deref(), Some("kept"));

        let io_err: Result<(), std::io::Error> = Err(std::io::Error::other("io"));
        assert_eq!(io_err.log_err(), None);
    }
}
