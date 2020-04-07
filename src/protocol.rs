use serde::Serialize;

#[derive(Default, Serialize)]
pub struct SwaybarHeader {
    pub version: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub click_events: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cont_signal: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_signal: Option<i32>,
}

#[derive(Default, Serialize)]
pub struct SwaybarBlock<'a> {
    pub full_text: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_text: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_top: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_left: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_right: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_width: Option<u8>, // Could also be a `String` but we don't need that
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub urgent: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separator: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separator_block_width: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markup: Option<&'a str>,
}
