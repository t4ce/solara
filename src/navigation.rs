//! Minimal address editor. The terminal adapter owns focus and key bindings.
use url::Url;

#[derive(Default)]
pub struct Address {
    pub text: String,
    /// UTF-8 byte offset, always on a character boundary.
    pub cursor: usize,
    pub http: bool,
    pub selected: bool,
}
impl Address {
    pub fn set_url(&mut self, url: &Url) {
        self.http = url.scheme() == "http";
        self.text = url
            .as_str()
            .split_once("://")
            .map_or(url.as_str(), |(_, rest)| rest)
            .into();
        self.cursor = self.text.len();
        self.selected = false;
    }
    pub fn select_all(&mut self) {
        self.selected = true;
        self.cursor = self.text.len();
    }
    pub fn insert(&mut self, text: &str) {
        if text.chars().any(char::is_control) {
            return;
        }
        let old_len = if self.selected { 0 } else { self.text.len() };
        if old_len + text.len() > 8192 {
            return;
        }
        self.clear_selection();
        self.text.insert_str(self.cursor, text);
        self.cursor += text.len();
    }
    fn clear_selection(&mut self) -> bool {
        if !self.selected {
            return false;
        }
        self.text.clear();
        self.cursor = 0;
        self.selected = false;
        true
    }
    pub fn left(&mut self) {
        self.selected = false;
        self.cursor = self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i);
    }
    pub fn right(&mut self) {
        self.selected = false;
        if let Some(ch) = self.text[self.cursor..].chars().next() {
            self.cursor += ch.len_utf8();
        }
    }
    pub fn backspace(&mut self) {
        if self.clear_selection() || self.cursor == 0 {
            return;
        }
        let end = self.cursor;
        self.left();
        self.text.replace_range(self.cursor..end, "");
    }
    pub fn delete(&mut self) {
        if self.clear_selection() || self.cursor == self.text.len() {
            return;
        }
        let start = self.cursor;
        self.right();
        self.text.replace_range(start..self.cursor, "");
        self.cursor = start;
    }
    pub fn url(&self) -> Result<Url, String> {
        let text = self.text.trim();
        if text.is_empty() {
            return Err("Enter a web address".into());
        }
        if text.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err("Use one URL; encode spaces as %20".into());
        }
        let input = if text.contains("://") {
            text.to_owned()
        } else {
            format!("{}://{text}", if self.http { "http" } else { "https" })
        };
        let url = Url::parse(&input).map_err(|e| format!("Invalid URL: {e}"))?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err("Only HTTP and HTTPS addresses are supported".into());
        }
        Ok(url)
    }
    pub fn toggle(&mut self) {
        // Normalize a pasted complete URL before changing the selected scheme.
        if self.text.contains("://")
            && let Ok(url) = self.url()
        {
            self.set_url(&url);
        }
        self.http = !self.http;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schemes_ports_paths_and_queries() {
        let mut a = Address::default();
        a.insert("localhost:8080/a/../b?q=one%20two#part");
        assert_eq!(
            a.url().unwrap().as_str(),
            "https://localhost:8080/b?q=one%20two#part"
        );
        a.toggle();
        assert_eq!(a.url().unwrap().scheme(), "http");
        a.select_all();
        a.insert("https://example.com:8443/path");
        a.toggle();
        assert_eq!(a.url().unwrap().as_str(), "http://example.com:8443/path");
    }
    #[test]
    fn editing_unicode_and_replacing_selection() {
        let mut a = Address::default();
        a.insert("aéz");
        a.left();
        a.backspace();
        assert_eq!(a.text, "az");
        a.delete();
        assert_eq!(a.text, "a");
        a.select_all();
        a.insert("example.com");
        assert_eq!(a.text, "example.com");
        a.select_all();
        a.backspace();
        assert!(a.text.is_empty());
    }
    #[test]
    fn rejects_empty_non_web_and_terminal_controls() {
        let mut a = Address::default();
        assert!(a.url().is_err());
        a.insert("file:///tmp/a");
        assert!(a.url().is_err());
        a.select_all();
        a.insert("good.test\x1b[2J");
        assert_eq!(a.text, "file:///tmp/a");
        a.insert("a b");
        assert!(a.url().is_err());
    }
}
