//! A yes/no question on the terminal.
//!
//! This file depends on nothing else in the crate, so the integration tests
//! compile it directly with `#[path]` and drive it on a pseudo-terminal.

use std::io;

use console::Term;
use dialoguer::Confirm;

/// Ask a yes/no question on the terminal and wait for an answer.
///
/// `default` is what Enter answers. Returns `Ok(None)` when the user pressed
/// Ctrl+C. Any other failure, including no terminal to ask on, is an error.
pub(crate) fn ask(prompt: &str, default: bool) -> io::Result<Option<bool>> {
    match Confirm::new()
        .with_prompt(prompt)
        .default(default)
        .interact()
    {
        Ok(answer) => Ok(Some(answer)),
        // On Unix, console reads Ctrl+C at a prompt as a byte, returns it as
        // `ErrorKind::Interrupted`, and raises SIGINT itself.
        Err(dialoguer::Error::IO(err)) if err.kind() == io::ErrorKind::Interrupted => Ok(None),
        Err(dialoguer::Error::IO(err)) => {
            // dialoguer hides the cursor for the prompt and shows it again
            // only after an answer, so a failed prompt would leave it hidden.
            let term = Term::stderr();
            if term.is_term() {
                let _ = term.show_cursor();
            }
            Err(err)
        }
    }
}
