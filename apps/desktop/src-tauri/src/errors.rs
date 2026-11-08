//! Turning backend errors into short sentences the UI can show as-is.

use phonesym_platform::Error;

/// A friendly message for the UI. The core error text is kept as detail.
pub fn friendly(e: Error) -> String {
    match e {
        Error::AdapterUnavailable(d) => format!("Bluetooth is off or no adapter was found. ({d})"),
        Error::DeviceNotFound(d) => format!("Phone not found. Is it nearby and paired? ({d})"),
        Error::NotConnected(d) => format!("Your phone is not connected. ({d})"),
        Error::ServiceUnavailable(d) => format!("A required system service is missing: {d}"),
        Error::InvalidState(d) => format!("Not possible right now: {d}"),
        Error::InvalidArgument(d) => format!("Invalid input: {d}"),
        Error::NotSupported(d) => format!("Not available yet on this system: {d}"),
        Error::Refused(d) => {
            format!("The phone refused. Check the sharing permissions in the phone's Bluetooth settings. ({d})")
        }
        Error::Backend(d) => format!("Something went wrong: {d}"),
    }
}

/// Shorthand used by every command: `core_result.map_err(friendly)`.
pub type CmdResult<T> = Result<T, String>;
