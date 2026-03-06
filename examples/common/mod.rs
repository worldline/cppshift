use std::{env, fs, io, path::PathBuf};

#[derive(Debug, Clone)]
pub(crate) struct Source(PathBuf);

impl Source {
    /// Get the path of the source file.
    pub(crate) fn to_str(&self) -> Option<&str> {
        self.0.to_str()
    }

    /// Read the source file into a string.
    pub(crate) fn read_to_string(&self) -> Result<String, io::Error> {
        fs::read_to_string(&self.0)
    }
}

pub(crate) struct Sources {
    args_iter: Box<dyn Iterator<Item = String>>,
    glob_iter: Option<glob::Paths>,
}

impl Sources {
    /// Get the next source file from the glob iterator
    fn next_source(&mut self) -> Option<Source> {
        if let Some(paths) = self.glob_iter.as_mut() {
            paths
                .filter_map(|path| {
                    path.ok().and_then(|p| {
                        if p.extension().is_some_and(|e| {
                            e.eq_ignore_ascii_case("h")
                                || e.eq_ignore_ascii_case("hpp")
                                || e.eq_ignore_ascii_case("c")
                                || e.eq_ignore_ascii_case("cpp")
                        }) {
                            Some(Source(p))
                        } else {
                            None
                        }
                    })
                })
                .next()
        } else {
            None
        }
    }
}

impl Default for Sources {
    fn default() -> Self {
        let mut args_iter = Box::new(env::args());
        args_iter.next(); // Skip the first argument, which is the program name
        Self {
            args_iter,
            glob_iter: None,
        }
    }
}

impl Iterator for Sources {
    type Item = Source;

    fn next(&mut self) -> Option<Self::Item> {
        if self.glob_iter.is_some() {
            if let Some(source) = self.next_source() {
                Some(source)
            } else if let Some(arg) = self.args_iter.next() {
                self.glob_iter = glob::glob(&arg).ok();
                self.next_source()
            } else {
                None
            }
        } else if let Some(arg) = self.args_iter.next() {
            self.glob_iter = glob::glob(&arg).ok();
            self.next_source()
        } else {
            None
        }
    }
}
