use dialoguer::{Confirm, Input, Password};

use crate::error::ZuulError;

/// Ask for a yes/no confirmation, respecting non-interactive mode.
///
/// In non-interactive mode, returns `Ok(true)` if `force` is set,
/// or returns an error telling the user to pass `--force`.
pub fn confirm(message: &str, force: bool, non_interactive: bool) -> Result<bool, ZuulError> {
    if force {
        return Ok(true);
    }
    if non_interactive {
        return Err(ZuulError::Validation(format!(
            "Confirmation required: {message}\nUse --force to skip confirmation in non-interactive mode."
        )));
    }
    Confirm::new()
        .with_prompt(message)
        .default(false)
        .interact()
        .map_err(|e| ZuulError::Config(format!("Failed to read input: {e}")))
}

/// Ask the user to type a specific string to confirm a destructive action.
///
/// In non-interactive mode, returns an error since typed confirmation
/// cannot be bypassed.
pub fn confirm_typed(
    prompt: &str,
    expected: &str,
    non_interactive: bool,
) -> Result<bool, ZuulError> {
    if non_interactive {
        return Err(ZuulError::Validation(
            "Typed confirmation required but running in non-interactive mode.".to_string(),
        ));
    }
    let typed: String = Input::new()
        .with_prompt(prompt)
        .interact_text()
        .map_err(|e| ZuulError::Config(format!("Failed to read input: {e}")))?;
    Ok(typed == expected)
}

/// Prompt for a text input with a message.
///
/// In non-interactive mode, returns an error.
pub fn input(message: &str, non_interactive: bool) -> Result<String, ZuulError> {
    if non_interactive {
        return Err(ZuulError::Validation(format!(
            "Input required: {message}\nProvide the value via command-line arguments in non-interactive mode."
        )));
    }
    Input::new()
        .with_prompt(message)
        .interact_text()
        .map_err(|e| ZuulError::Config(format!("Failed to read input: {e}")))
}

/// Prompt for a secret value (hidden input).
///
/// In non-interactive mode, returns an error.
pub fn password(message: &str, non_interactive: bool) -> Result<String, ZuulError> {
    if non_interactive {
        return Err(ZuulError::Validation(format!(
            "Input required: {message}\nUse --from-file or --from-stdin to provide the value in non-interactive mode."
        )));
    }
    Password::new()
        .with_prompt(message)
        .interact()
        .map_err(|e| ZuulError::Config(format!("Failed to read input: {e}")))
}
