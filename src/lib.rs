use std::process::Command;
use std::io;
use std::str::FromStr;
use std::fmt;

/// The result of a lint check.
#[derive(Debug)]
pub enum Check {
    /// Not problems were found.
    Perfect,
    /// These warnings were found.
    Warning(Vec<ProblemDescription>),
    /// These errors were found.
    Error(Vec<ProblemDescription>),
}
unsafe impl Send for Check {}

/// A problem found when using the cargo check linter.
#[derive(Debug)]
pub enum CheckError {
    InvalidDirectory,
    IoError(io::Error),
}
impl From<io::Error> for CheckError {
    fn from(err: io::Error) -> CheckError {
        CheckError::IoError(err)
    }
}

/// A note about a span in the source file.
#[derive(Debug, Clone)]
pub struct Note {
    pub start_line: usize,
    /// In characters.
    pub start_col: usize,
    pub end_line: usize,
    /// In characters.
    pub end_col: usize,
    pub message: String,
}

impl Note {
    pub fn new<T>(start_line: usize, start_col: usize, end_line: usize, 
            end_col: usize, message: T) 
            -> Note 
            where T: Into<String> {
        Note {
            start_line: start_line, start_col: start_col, end_line: end_line,
            end_col: end_col, message: message.into()
        }
    }
}

impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{}:{}: {}:{}: {}", self.start_line, self.start_col, 
            self.end_line, self.end_col, self.message
        )
    }
}

/// A problem found in the code of a file during linting.
#[derive(Debug, Clone)]
pub struct ProblemDescription {
    pub filepath: String,
    pub message: Note,
    pub help: Vec<Note>,
    pub notes: Vec<Note>,
}

impl ProblemDescription {
    /// Creates a new problem description.
    pub fn new<T, N>(filepath: T, start_line: usize, start_col: usize, end_line: usize, 
            end_col: usize, message: T, help: N, notes: N) 
            -> ProblemDescription 
            where T: Into<String>, N: Into<Vec<Note>> {
        let message = Note::new(start_line, start_col, end_line, end_col, message);
        ProblemDescription {
            filepath: filepath.into(),
            message: message,
            help: help.into(),
            notes: notes.into(),
        }
    }
}

impl fmt::Display for ProblemDescription {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        try!(write!(f, "{}:", self.filepath));
        try!(fmt::Display::fmt(&self.message, f));
        for help in &self.help {
            try!(write!(f, " (help: "));
            try!(fmt::Display::fmt(help, f));
            try!(write!(f, ")"));
        }
        for note in &self.notes {
            try!(write!(f, " (note: "));
            try!(fmt::Display::fmt(note, f));
            try!(write!(f, ")"));
        }
        Ok(())
    }
}

const COMPILE_ERROR_LINE: &'static str = "error: aborting due to previous error";

#[derive(Debug)]
enum LineParseState {
    File,
    StartLine,
    StartCol,
    EndLine,
    EndCol,
    Level,
    FirstMessageLine,
}

#[derive(Debug)]
enum Level {
    Warning,
    Error,
    Help,
    Note,
}

fn parse_check_line(line: &str) -> (Level, ProblemDescription) {
    use self::LineParseState::*;
    let mut state = File;
    let mut filepath = String::new();
    let mut start_line = 0;
    let mut start_col = 0;
    let mut end_line = 0;
    let mut end_col = 0;
    let mut level = self::Level::Warning;
    
    let mut start = 0;
    for (i, ch) in line.char_indices() {
        match state {
            File => {
                if ch == ':' {
                    filepath = String::from(&line[0..i]);
                    state = StartLine;
                    start = i + ch.len_utf8();
                }
            },
            StartLine => {
                if ch == ':' {
                    start_line = usize::from_str(&line[start..i])
                        .expect("Invalid start line from cargo check");
                    state = StartCol;
                    start = i + ch.len_utf8();
                }
            },
            StartCol => {
                if ch == ':' {
                    start_col = usize::from_str(&line[start..i])
                        .expect("Invalid start col from cargo check");
                    state = EndLine;
                    start = i + ch.len_utf8();
                }
            },
            EndLine => {
                if ch.is_whitespace() {
                    start = i + ch.len_utf8();
                } else if ch == ':' {
                    end_line = usize::from_str(&line[start..i])
                        .expect("Invalid end line from cargo check");
                    state = EndCol;
                    start = i + ch.len_utf8();
                }
            },
            EndCol => {
                if ch.is_whitespace() {
                    end_col = usize::from_str(&line[start..i])
                        .expect("Invalid end col from cargo check");
                    state = Level;
                    start = i + ch.len_utf8();
                }
            },
            Level => {
                if ch.is_whitespace() {
                    start = i + ch.len_utf8();
                } else if ch == ':' {
                    let level_text = &line[start..i];
                    match level_text {
                        "warning" => {},
                        "error" => {
                            level = self::Level::Error;
                        },
                        "help" => {
                            level = self::Level::Help;
                        },
                        "note" => {
                            level = self::Level::Note;
                        },
                        l => panic!("Unknown error warning level: {}", l),
                    }
                    state = FirstMessageLine;
                }
            },
            FirstMessageLine => {
                if ch.is_whitespace() {
                    start = i + ch.len_utf8();
                } else {
                    let problem = ProblemDescription::new(filepath, start_line, 
                        start_col, end_line, end_col, 
                        String::from(&line[start..]), vec![], vec![],
                    );
                    return (level, problem);
                }
            }
        }
    }
    panic!("The line could not be parsed! '{}'", line);
}

fn line_is_visual_aid(line: &str) -> bool {
    let mut file_found = false;
    let mut line_found = false;
    let mut col_found = false;
    for ch in line.chars() {
        if ch.is_whitespace() {
            return ! col_found;
        }
        if ch == ':' {
            if ! file_found {
                file_found = true;
            } else if ! line_found {
                line_found = true;
            } else if ! col_found {
                col_found = true;
            } else {
                return false;
            }
        }
    }
    return false
}

/// Runs the ```cargo check``` linter on the current directory and returns
/// descriptions of the found problems.
pub fn cargo_check() -> Result<Check, CheckError> {
    use self::Check::*;
    
    let mut problems = Vec::new();
    let output = try!(Command::new("cargo").arg("check").output());
    if ! output.status.success() {
        return Err(CheckError::InvalidDirectory);
    }
    let mut is_warning = true;
    
    //println!("Stderr:");
    let stderr_text = String::from_utf8(output.stderr)
        .expect("Invalid UTF-8 returned by cargo check");
    let mut lines = stderr_text.lines();
    let mut cur_line = lines.next();
    
    while let Some(line) = cur_line {
        if line == COMPILE_ERROR_LINE {
            break;
        }
        println!("Current line: '{}'", line);
        // Parse the problem on the current line
        let (level, mut problem) = parse_check_line(line);
        
        // Check for more info in the following lines
        let mut was_visual = false;
        cur_line = lines.next();
        while let Some(line) = cur_line {
            if ! line_is_visual_aid(line) {
                // This is a new message
                if line.starts_with(&problem.filepath) {
                    break;
                } else if was_visual {
                    continue;
                } else {
                    problem.message.message.push('\n');
                    problem.message.message.push_str(line);
                }
            } else {
                was_visual = true;
            }
            cur_line = lines.next();
        }
        
        // Find out how to use the found problem
        match level {
            Level::Error => {
                is_warning = false;
                problems.push(problem);
            },
            // Add this help message to the previous problem
            Level::Help => {
                let last = problems.len() - 1;
                let mut last_problem = &mut problems[last];
                last_problem.help.push(problem.message);
            },
            Level::Warning => {
                problems.push(problem);
                // Ignore the visual error location lines
                lines.next();
                lines.next();
            },
            // Add this help note to the previous problem
            Level::Note => {
                let last = problems.len() - 1;
                let mut last_problem = &mut problems[last];
                last_problem.notes.push(problem.message);
            },
        }
    }
    /*for (num, line) in stderr_text.lines().enumerate() {
        println!("{}: {}", num, line);
    }*/
    Ok(if is_warning {
        if problems.is_empty() {
            Perfect
        } else {
            Warning(problems)
        }
    } else {
        Error(problems)
    })
}


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}
