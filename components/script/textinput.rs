/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Common handling of keyboard input and state management for text input controls

use dom::bindings::codegen::Bindings::KeyboardEventBinding::KeyboardEventMethods;
use dom::bindings::js::JSRef;
use dom::keyboardevent::KeyboardEvent;
use servo_util::str::DOMString;

use std::cmp::{min, max};
use std::default::Default;

#[jstraceable]
struct TextPoint {
    line: uint,
    index: uint,
}

#[jstraceable]
pub struct TextInput {
    /// Current text input content, split across lines without trailing '\n'
    lines: Vec<DOMString>,
    /// Current cursor input point
    edit_point: TextPoint,
    /// Selection range, beginning and end point that can span multiple lines.
    _selection: Option<(TextPoint, TextPoint)>,
    /// Is this ia multiline input?
    multiline: bool,
}

pub enum KeyReaction {
    TriggerDefaultAction,
    DispatchInput,
    Nothing,
}

impl Default for TextPoint {
    fn default() -> TextPoint {
        TextPoint {
            line: 0,
            index: 0,
        }
    }
}

impl TextInput {
    pub fn new(multiline: bool, initial: DOMString) -> TextInput {
        let mut i = TextInput {
            lines: vec!(),
            edit_point: Default::default(),
            _selection: None,
            multiline: multiline,
        };
        i.set_content(initial);
        i
    }

    fn get_current_line(&self) -> &DOMString {
        &self.lines[self.edit_point.line]
    }

    fn insert_char(&mut self, ch: char) {
        //TODO: handle replacing selection with character
        let new_line = {
            let prefix = self.get_current_line().as_slice().slice_chars(0, self.edit_point.index);
            let suffix = self.get_current_line().as_slice().slice_chars(self.edit_point.index,
                                                                        self.current_line_length());
            let mut new_line = prefix.to_string();
            new_line.push_char(ch);
            new_line.append(suffix.as_slice())
        };
        *self.lines.get_mut(self.edit_point.line) = new_line;
        self.edit_point.index += 1;
    }

    fn delete_char(&mut self, forward: bool) {
        //TODO: handle deleting selection
        let prefix_end = if forward {
            self.edit_point.index
        } else {
            //TODO: handle backspacing from position 0 of current line
            if self.multiline {
                assert!(self.edit_point.index > 0);
            } else if self.edit_point.index == 0 {
                return;
            }
            self.edit_point.index - 1
        };
        let suffix_start = if forward {
            let is_eol = self.edit_point.index == self.current_line_length() - 1;
            if self.multiline {
                //TODO: handle deleting from end position of current line
                assert!(!is_eol);
            } else if is_eol {
                return;
            }
            self.edit_point.index + 1
        } else {
            self.edit_point.index
        };

        let new_line = {
            let prefix = self.get_current_line().as_slice().slice_chars(0, prefix_end);
            let suffix = self.get_current_line().as_slice().slice_chars(suffix_start,
                                                                        self.current_line_length());
            let new_line = prefix.to_string();
            new_line.append(suffix)
        };
        *self.lines.get_mut(self.edit_point.line) = new_line;

        if !forward {
            self.adjust_horizontal(-1);
        }
    }

    fn current_line_length(&self) -> uint {
        self.lines[self.edit_point.line].len()
    }

    fn adjust_vertical(&mut self, adjust: int) {
        if !self.multiline {
            return;
        }

        if adjust < 0 && self.edit_point.line as int + adjust < 0 {
            self.edit_point.index = 0;
            self.edit_point.line = 0;
            return;
        } else if adjust > 0 && self.edit_point.line >= min(0, self.lines.len() - adjust as uint) {
            self.edit_point.index = self.current_line_length();
            self.edit_point.line = self.lines.len() - 1;
            return;
        }

        self.edit_point.line = (self.edit_point.line as int + adjust) as uint;
        self.edit_point.index = min(self.current_line_length(), self.edit_point.index);
    }

    fn adjust_horizontal(&mut self, adjust: int) {
        if adjust < 0 {
            if self.multiline {
                let remaining = self.edit_point.index;
                if adjust.abs() as uint > remaining {
                    self.edit_point.index = 0;
                    self.adjust_vertical(-1);
                    self.edit_point.index = self.current_line_length();
                    self.adjust_horizontal(adjust + remaining as int);
                } else {
                    self.edit_point.index = (self.edit_point.index as int + adjust) as uint;
                }
            } else {
                self.edit_point.index = max(0, self.edit_point.index as int + adjust) as uint;
            }
        } else {
            if self.multiline {
                let remaining = self.current_line_length() - self.edit_point.index;
                if adjust as uint > remaining {
                    self.edit_point.index = 0;
                    self.adjust_vertical(1);
                    self.adjust_horizontal(adjust - remaining as int);
                } else {
                    self.edit_point.index += adjust as uint;
                }
            } else {
                self.edit_point.index = min(self.current_line_length(),
                                            self.edit_point.index + adjust as uint);
            }
        }
    }

    fn handle_return(&mut self) -> KeyReaction {
        if !self.multiline {
            return TriggerDefaultAction;
        }

        //TODO: support replacing selection with newline
        let prefix = self.get_current_line().as_slice().slice_chars(0, self.edit_point.index).to_string();
        let suffix = self.get_current_line().as_slice().slice_chars(self.edit_point.index,
                                                                    self.current_line_length()).to_string();
        *self.lines.get_mut(self.edit_point.line) = prefix;
        self.lines.insert(self.edit_point.line + 1, suffix);
        return DispatchInput;
    }

    pub fn handle_keydown(&mut self, event: JSRef<KeyboardEvent>) -> KeyReaction {
        match event.Key().as_slice() {
            c if c.len() == 1 => {
                self.insert_char(c.char_at(0));
                return DispatchInput;
            }
            "Space" => {
                self.insert_char(' ');
                DispatchInput
            }
            "Delete" => {
                self.delete_char(true);
                DispatchInput
            }
            "Backspace" => {
                self.delete_char(false);
                DispatchInput
            }
            "ArrowLeft" => {
                self.adjust_horizontal(-1);
                Nothing
            }
            "ArrowRight" => {
                self.adjust_horizontal(1);
                Nothing
            }
            "ArrowUp" => {
                self.adjust_vertical(-1);
                Nothing
            }
            "ArrowDown" => {
                self.adjust_vertical(1);
                Nothing
            }
            "Enter" => self.handle_return(),
            "Home" => {
                self.edit_point.index = 0;
                Nothing
            }
            "End" => {
                self.edit_point.index = self.current_line_length();
                Nothing
            }
            "Tab" => TriggerDefaultAction,
            _ => Nothing,
        }
    }

    pub fn get_content(&self) -> DOMString {
        let mut content = "".to_string();
        for (i, line) in self.lines.iter().enumerate() {
            content = content.append(line.as_slice());
            if i < self.lines.len() - 1 {
                content.push_char('\n');
            }
        }
        content
    }

    pub fn set_content(&mut self, content: DOMString) {
        self.lines = if self.multiline {
            content.as_slice().split('\n').map(|s| s.to_string()).collect()
        } else {
            vec!(content)
        };
        self.edit_point.line = min(self.edit_point.line, self.lines.len() - 1);
        self.edit_point.index = min(self.edit_point.index, self.current_line_length() - 1);
    }
}
