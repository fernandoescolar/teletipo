pub trait Clipboard {
    fn get(&self) -> Option<String>;
    fn set(&mut self, text: String);
}

pub trait Ime {
    fn begin_composition(&mut self);
    fn update_preedit(&mut self, preedit: &str);
    fn commit(&mut self) -> Option<String>;
    fn cancel(&mut self);
}

pub trait Accessibility {
    fn announce(&self, text: &str);
    fn set_focus(&mut self, node_id: &str);
}

pub trait DpiAwareness {
    fn scale_factor(&self) -> f32;
    fn logical_to_physical(&self, value: f32) -> f32 {
        value * self.scale_factor()
    }
}

pub trait FontFallback {
    fn fallback_for_char(&self, ch: char) -> Option<String>;
}
