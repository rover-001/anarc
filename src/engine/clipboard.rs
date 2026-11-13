//! System clipboard bridge for Linux (Wayland & X11).

pub fn write_system_clipboard(text: &str) {
    write_clipboard_internal(text, false);
    write_clipboard_internal(text, true);
}

pub fn write_primary_clipboard(text: &str) {
    write_clipboard_internal(text, true);
}

fn write_clipboard_internal(text: &str, primary: bool) {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        let mut cmd = std::process::Command::new("wl-copy");
        if primary {
            cmd.arg("--primary");
        }
        if let Ok(mut child) = cmd.stdin(std::process::Stdio::piped()).spawn() {
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
            return;
        }
    }

    if std::env::var_os("DISPLAY").is_some() {
        let selection = if primary { "primary" } else { "clipboard" };
        if let Ok(mut child) = std::process::Command::new("xclip")
            .args(["-selection", selection])
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
        }
    }
}

pub fn read_system_clipboard() -> Option<String> {
    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        && let Ok(output) = std::process::Command::new("wl-paste")
            .arg("--no-newline")
            .output()
        && output.status.success()
        && !output.stdout.is_empty()
        && let Ok(text) = String::from_utf8(output.stdout)
    {
        return Some(text);
    }

    if std::env::var_os("DISPLAY").is_some()
        && let Ok(output) = std::process::Command::new("xclip")
            .args(["-selection", "clipboard", "-o"])
            .output()
        && output.status.success()
        && !output.stdout.is_empty()
        && let Ok(text) = String::from_utf8(output.stdout)
    {
        return Some(text);
    }

    None
}

pub fn url_decode(s: &str) -> String {
    let mut bytes = Vec::new();
    let mut chars = s.as_bytes().iter();
    while let Some(&b) = chars.next() {
        if b == b'%'
            && let (Some(&h1), Some(&h2)) = (chars.next(), chars.next())
        {
            let hex_slice = [h1, h2];
            if let Ok(hex_str) = std::str::from_utf8(&hex_slice)
                && let Ok(val) = u8::from_str_radix(hex_str, 16)
            {
                bytes.push(val);
                continue;
            }
        }
        bytes.push(b);
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_decode() {
        assert_eq!(url_decode("Hello%20World%21"), "Hello World!");
        assert_eq!(url_decode("Line1%0ALine2"), "Line1\nLine2");
        assert_eq!(url_decode("a%2Bb%3Dc"), "a+b=c");
    }
}
