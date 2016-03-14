
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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
    /// Move to first position on next line. If that position is on the margin, scroll up.
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
    /// Move the cursor n tabs backward (CBT).
    CursorBackwardsTabulation(usize),
    /// Move the cursor to the nth column, 0-indexed (CHA).
    CursorHorizontalAbsolute(usize),
    /// Move the cursor n tabs forward (CHT).
    CursorForwardTabulation(usize),
    /// Move the cursor down n lines, placing it in the first column (CNL).
    CursorNextLine(usize),
    /// Move the cursor up n lines, placing it in the first column (CPL).
    CursorPreviousLine(usize),
    /// Report cursor position (CPR).
    ///
    /// Report format: ``ESC [ r ; c R`` where ``r`` and ``c`` are current row and column.
    CursorPositionReport,
    /// Set cursor position, zero-indexed row-column (CUP).
    CursorPosition(usize, usize),
    /// Move cursor position n columns forward (CUF), stopping at right border of page.
    CursorForward(usize),
    /// Move cursor position n columns backward (CUB), stopping at left border of page.
    CursorBackward(usize),
    /// Move cursor position n rows down (CUD), stopping at the bottom line.
    CursorDown(usize),
    /// Move cursor position n rows up (CUU), stopping at the top line.
    CursorUp(usize),
    /// Delete n characters from the cursor position to the right (DCH).
    ///
    /// If n is larger than the number of characters between the cursor and the right margin, only
    /// delete to the right margin.
    ///
    /// Characters not deleted should move to the left to fill the positions of the deleted
    /// characters, keeping their original character attributes.
    DeleteCharacter(usize),
    /// Delete n lines, default = 1.
    DeleteLines(usize),
    /// Insert n lines, default = 1.
    InsertLines(usize),
    /// Sets the scrolling region (top, bottom), defaults to whole window.
    ScrollingRegion(usize, usize),
    SendPrimaryDeviceAttributes,
    SendSecondaryDeviceAttributes,
    /* OSC */
    SetWindowTitle(String),
    SetIconName(String),
    SetXProps(String),
    SetColorNumber(String),
    LinePositionAbsolute(usize),
}

#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum KeypadMode {
    Numeric,
    Application,
}

#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PrivateMode {
    /// Application Cursor Keys (DECCKM).
    ///
    /// * If the DECCKM function is set, then the arrow keys send application sequences to the host.
    /// * If the DECCKM function is reset, then the arrow keys send ANSI cursor sequences to the host.
    ApplicationCursorKeys,
    /// Sets/clears the key-repeat, when set keys are to be repeated after 0.5 seconds until key is
    /// released (DECARM).
    ///
    /// Default: repeat (set).
    Autorepeat,
    /// Sets/clears the automatic wrapping of the cursor when it reaches the end of a line (DECAWM)
    ///
    /// Default: No-autowrap (not set).
    Autowrap,
    /// Start/stop blinking cursor, att610
    ///
    /// Default: on
    CursorBlink,
    /// Default: on
    ShowCursor,
    /// Default: off
    AlternateScreenBuffer,
    /// Save cursor as in DECSC
    SaveCursor,
    /// Save cursor, switch to alternate screen buffer, clearing it first.
    ///
    /// This combines ``AlternateScreenBuffer`` and ``SaveCursor``.
    SaveCursorAlternateBufferClear,
    /// If set the background is light with dark letters, if not set background is dark with light
    /// letters.
    ///
    /// Default: off
    LightScreen,
    /// If set send focus events when terminal gains or loses focus.
    ///
    /// "\x1B[I" for focus in and "\x1B[O" for focus out.
    ///
    /// Default: off
    SendFocusEvents,
}

#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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

#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EraseInLine {
    Left,
    Right,
    All,
}

#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EraseInDisplay {
    Above,
    Below,
    All,
}

#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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

#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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

#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CharAttr {
    Reset,
    Set(CharType),
    Unset(CharType),
    FGColor(Color),
    BGColor(Color),
}

#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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

#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CharsetIndex {
    G0,
    G1,
    G2,
    G3,
}
