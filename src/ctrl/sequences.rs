
#[derive(Debug, Eq, Hash, PartialEq)]
pub enum Seq {
    /* Single character functions */
    Bell,
    Backspace,
    CarriageReturn,
    ReturnTerminalStatus,
    FormFeed,
    LineFeed,
    ShiftIn,
    ShiftOut,
    Tab,
    TabVertical,

    Unicode(u32),

    Index,
    NextLine,
    TabSet,
    ReverseIndex,
    SingleShiftSelectG2CharSet,
    SingleShiftSelectG3CharSet,
    DeviceControlString,
    StartOfGuardedArea,
    EndOfGuardedArea,
    StartOfString,
    ReturnTerminalId,
    StringTerminator,
    PrivacyMessage,
    ApplicationProgramCommand,

    Charset(CharsetIndex, Charset),

    SetKeypadMode(KeypadMode),

    /* CSI */
    ModeSet(Vec<Mode>),
    ModeReset(Vec<Mode>),
    PrivateModeSet(Vec<PrivateMode>),
    PrivateModeReset(Vec<PrivateMode>),
    CharAttr(Vec<CharAttr>),
    EraseInLine(EraseInLine),
    EraseInDisplay(EraseInDisplay),
    /// Set cursor position, zero-indexed row-column
    CursorPosition(usize, usize),
    /* OSC */
    SetWindowTitle(String),
    SetIconName(String),
    SetXProps(String),
    SetColorNumber(String),
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum KeypadMode {
    Numeric,
    Application,
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum PrivateMode {
    /// Application Cursor Keys (DECCKM).
    ApplicationCursorKeys,
    /// Represents an unknown PrivateMode
    Unknown(u32),
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum Mode {
    /// AM
    KeyboardAction,
    /// IRM
    Insert,
    /// SRM
    SendReceive,
    /// LNM
    AutomaticNewline,
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum EraseInLine {
    Left,
    Right,
    All,
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum EraseInDisplay {
    Above,
    Below,
    All,
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum CharType {
    Normal,
    Bold,
    Faint,

    Italicized,
    Underlined,
    Blink,
    Inverse,
    Invisible,
    CrossedOut,
    DoublyUnderlined,
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum Color {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    Default,
    Palette(u8),
    RGB(u8, u8, u8)
}

impl Default for Color {
    fn default() -> Color {
        Color::Default
    }
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum CharAttr {
    Reset,
    Set(CharType),
    Unset(CharType),
    FGColor(Color),
    BGColor(Color),
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum Charset {
    DECSpecialAndLineDrawing,
    DECSupplementary,
    DECSupplementaryGraphics,
    DECTechnical,
    UnitedKingdom,
    UnitedStates,
    Dutch,
    Finnish,
    French,
    FrenchCanadian,
    German,
    Italian,
    NorwegianDanish,
    Portuguese,
    Spanish,
    Swedish,
    Swiss,
    // Unicode,
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum CharsetIndex {
    G0,
    G1,
    G2,
    G3,
}
