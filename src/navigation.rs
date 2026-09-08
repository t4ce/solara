//! Minimal address editor. The terminal adapter owns focus and key bindings.
use url::Url;

/// A page target selected from the navigator's address field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavigationTarget {
    Web(Url),
    Demo(BuiltInDemo),
}

impl std::fmt::Display for NavigationTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Web(url) => url.fmt(f),
            Self::Demo(demo) => demo.fmt(f),
        }
    }
}

/// The bounded corpus available in the single Solara browser tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltInDemo {
    TextAndBorders,
    DivsAndPanels,
    FlowAndForms,
}

impl BuiltInDemo {
    pub const fn address(self) -> &'static str {
        match self {
            Self::TextAndBorders => "demo1",
            Self::DivsAndPanels => "demo2",
            Self::FlowAndForms => "demo3",
        }
    }
}

impl std::fmt::Display for BuiltInDemo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.address())
    }
}

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
    pub fn target(&self) -> Result<NavigationTarget, String> {
        let demo = match self.text.trim().to_ascii_lowercase().as_str() {
            "demo1" => Some(BuiltInDemo::TextAndBorders),
            "demo2" => Some(BuiltInDemo::DivsAndPanels),
            "demo3" => Some(BuiltInDemo::FlowAndForms),
            _ => None,
        };
        Ok(match demo {
            Some(demo) => NavigationTarget::Demo(demo),
            None => NavigationTarget::Web(self.url()?),
        })
    }
    pub fn set_demo(&mut self, demo: BuiltInDemo) {
        self.text = demo.address().into();
        self.cursor = self.text.len();
        self.selected = false;
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
    #[test]
    fn recognizes_the_three_single_tab_builtins() {
        let mut a = Address::default();
        for (address, expected) in [
            ("demo1", BuiltInDemo::TextAndBorders),
            ("DEMO2", BuiltInDemo::DivsAndPanels),
            (" demo3 ", BuiltInDemo::FlowAndForms),
        ] {
            a.select_all();
            a.insert(address);
            assert_eq!(a.target().unwrap(), NavigationTarget::Demo(expected));
        }
        a.set_demo(BuiltInDemo::TextAndBorders);
        assert_eq!(a.text, "demo1");
        assert_eq!(a.cursor, 5);
    }
}

/// Number keys are available outside address editing. Picking never navigates.
#[derive(Default)]
pub struct Bookmarks {
    pub entries: Vec<(String, NavigationTarget)>,
    pub editing: bool,
}
impl Bookmarks {
    pub fn from_startup(json: &str) -> Result<Self, String> {
        let config: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let mut list = Self::default();
        if let Some(entries) = config["solara"]["bookmarks"].as_array() {
            // Positions are stable: an invalid entry is an error, not a renumbering.
            for entry in entries.iter().take(9) {
                let raw = entry
                    .as_str()
                    .or_else(|| entry["url"].as_str())
                    .ok_or("Bookmark needs a URL")?;
                if raw.len() > 8192 || raw.chars().any(|c| c.is_control() || c.is_whitespace()) {
                    return Err("Invalid bookmark URL".into());
                }
                let mut address = Address::default();
                address.insert(raw);
                let target = address.target()?;
                let label = entry["label"].as_str().unwrap_or(raw);
                list.entries.push((
                    label.chars().filter(|c| !c.is_control()).take(80).collect(),
                    target,
                ));
            }
        }
        Ok(list)
    }
    pub fn pick(&mut self, index: usize, address: &mut Address) -> bool {
        let Some((_, target)) = self.entries.get(index) else {
            return false;
        };
        match target {
            NavigationTarget::Web(url) => address.set_url(url),
            NavigationTarget::Demo(demo) => address.set_demo(*demo),
        }
        self.editing = false;
        true
    }
    pub fn character(&mut self, c: char, address: &mut Address) -> bool {
        if !self.editing && ('1'..='9').contains(&c) {
            return self.pick(c as usize - '1' as usize, address);
        }
        self.editing = true;
        address.insert(&c.to_string());
        false
    }
}
#[cfg(test)]
mod bookmark_tests {
    use super::*;
    #[test]
    fn capped_selection_preserves_http_and_editing_accepts_ip_digits() {
        let json = serde_json::json!({"solara":{"bookmarks": vec![
            serde_json::json!({"label":"BIOS", "url":"http://192.168.178.94:8338/"}); 12
        ]}});
        let mut list = Bookmarks::from_startup(&json.to_string()).unwrap();
        assert_eq!(list.entries.len(), 9);
        let mut address = Address::default();
        assert!(list.character('9', &mut address));
        assert_eq!(
            address.url().unwrap().as_str(),
            "http://192.168.178.94:8338/"
        );
        address.select_all();
        list.editing = true;
        for c in "192.168.178.94:8338/".chars() {
            assert!(!list.character(c, &mut address));
        }
        assert_eq!(
            address.url().unwrap().as_str(),
            "http://192.168.178.94:8338/"
        );
        assert!(!list.pick(9, &mut address));
    }
    #[test]
    fn missing_and_invalid_config_are_distinct() {
        assert!(Bookmarks::from_startup("{}").unwrap().entries.is_empty());
        assert!(
            Bookmarks::from_startup(r#"{"solara":{"bookmarks":["file:///private"]}}"#).is_err()
        );
        assert!(Bookmarks::from_startup(r#"{"solara":{"bookmarks":["http://a/\n"]}}"#).is_err());
    }
}
