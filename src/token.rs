use std::ffi::OsStr;

pub struct Tokenizer {
    args: Box<dyn Iterator<Item = &'static OsStr>>,
    pending: &'static str,
    pending_index: usize,
    rest_are_args: bool,
}

#[derive(Debug, PartialEq)]
pub enum Token {
    Short(char),
    Long(&'static str),
    LongEq(&'static str, &'static str),
    Arg(&'static OsStr),
}

impl Tokenizer {
    /// Creates a `Tokenizer` that iterates over `argv::iter()`, skipping the
    /// first item.
    pub(crate) fn new() -> Self {
        let mut args = argv::iter();

        // Skip the executable.
        let _ = args.next();

        Self::new_with_iterator(args)
    }

    /// Creates a `Tokenizer` that iterates over the `args`, processing all items.
    pub(crate) fn new_with_iterator<T: Iterator<Item = &'static OsStr> + 'static>(args: T) -> Self {
        Tokenizer {
            args: Box::new(args),
            pending: "",
            pending_index: 0,
            rest_are_args: false,
        }
    }

    pub fn next(&mut self) -> Option<Token> {
        if self.pending_index < self.pending.len() {
            let ch = self.pending[self.pending_index..].chars().next().unwrap();
            self.pending_index += ch.len_utf8();
            return Some(Token::Short(ch));
        }

        let arg = self.args.next()?;
        if self.rest_are_args {
            return Some(Token::Arg(arg));
        }

        let string = match arg.to_str() {
            Some(string) => string,
            None => return Some(Token::Arg(arg)),
        };

        if string == "--" {
            self.rest_are_args = true;
            return self.args.next().map(Token::Arg);
        }

        if string.starts_with("--") {
            return match string.find('=') {
                // "--foo bar" case
                None => Some(Token::Long(&string[2..])),
                // "--foo=bar" case
                Some(i) => {
                    let flag = &string[2..i];
                    let arg = &string[i + 1..];
                    Some(Token::LongEq(flag, arg))
                }
            };
        }

        if string.starts_with('-') && string != "-" {
            let ch = string[1..].chars().next().unwrap();
            self.pending = string;
            self.pending_index = 1 + ch.len_utf8();
            return Some(Token::Short(ch));
        }

        Some(Token::Arg(OsStr::new(string)))
    }

    pub fn next_arg(&mut self) -> Option<&'static OsStr> {
        if self.pending_index < self.pending.len() {
            let rest = &self.pending[self.pending_index..];
            self.pending_index = self.pending.len();
            Some(OsStr::new(rest))
        } else {
            self.args.next()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Token;
    use super::Tokenizer;
    use std::ffi::OsStr;

    /// The arguments should be an iterator of `std::ffi::OsStr`. These can be
    /// created by calling `as_ref()` on a `str`, which gets tedious pretty
    /// quickly. So put them in a `[&str]` and pass it to this function to
    /// get back an iterator that converts each to a ref as needed.
    fn args_as_ref<'a>(args: &'a [&str]) -> Box<dyn Iterator<Item = &'a OsStr> + 'a> {
        Box::new(args.iter().map(|s| s.as_ref()))
    }

    /// `-a` should work.
    #[test]
    fn one_short_flag_no_arg() {
        let args = args_as_ref(&["-a"]);
        let mut tokenizer = Tokenizer::new_with_iterator(args.into_iter());
        assert_eq!(tokenizer.next().unwrap(), Token::Short('a'));
    }

    /// `-a -b` should work.
    #[test]
    fn two_short_flags_no_args() {
        let args = args_as_ref(&["-a", "-b"]);
        let mut tokenizer = Tokenizer::new_with_iterator(args.into_iter());
        assert_eq!(tokenizer.next().unwrap(), Token::Short('a'));
        assert_eq!(tokenizer.next().unwrap(), Token::Short('b'));
    }

    /// `-ab` when `-a` takes no args should be treated as `-a -b`.
    #[test]
    fn two_short_flags_no_args_cuddled() {
        let args = args_as_ref(&["-ab"]);
        let mut tokenizer = Tokenizer::new_with_iterator(args.into_iter());
        assert_eq!(tokenizer.next().unwrap(), Token::Short('a'));
        assert_eq!(tokenizer.next().unwrap(), Token::Short('b'));
    }

    /// `-a b` should treat `b` as an arg for `-a`.
    #[test]
    fn one_short_flag_one_arg() {
        let args = args_as_ref(&["-a", "b"]);
        let mut tokenizer = Tokenizer::new_with_iterator(args.into_iter());
        assert_eq!(tokenizer.next().unwrap(), Token::Short('a'));
        assert_eq!(tokenizer.next_arg().unwrap(), "b");
    }

    /// `-ab` when `-a` takes an arg should be treated as `-a b`
    #[test]
    fn one_short_flag_one_arg_cuddled() {
        let args = args_as_ref(&["-ab"]);
        let mut tokenizer = Tokenizer::new_with_iterator(args.into_iter());
        assert_eq!(tokenizer.next().unwrap(), Token::Short('a'));
        assert_eq!(tokenizer.next_arg().unwrap(), "b");
    }

    /// `-a` when `-a` expects an arg should fail
    #[test]
    fn one_short_flag_missing_arg() {
        let args = args_as_ref(&["-a"]);
        let mut tokenizer = Tokenizer::new_with_iterator(args.into_iter());
        assert_eq!(tokenizer.next().unwrap(), Token::Short('a'));
        assert!(tokenizer.next_arg().is_none());
    }

    /// Long flag
    #[test]
    fn one_long_flag_no_args() {
        let args = args_as_ref(&["--foo"]);
        let mut tokenizer = Tokenizer::new_with_iterator(args.into_iter());
        assert_eq!(tokenizer.next().unwrap(), Token::Long("foo"));
    }

    /// `--foo bar` should work
    #[test]
    fn one_long_flag_one_arg_space() {
        let args = args_as_ref(&["--foo", "bar"]);
        let mut tokenizer = Tokenizer::new_with_iterator(args.into_iter());
        assert_eq!(tokenizer.next().unwrap(), Token::Long("foo"));
        assert_eq!(tokenizer.next_arg().unwrap(), "bar");
    }

    /// `--foo=bar` should work
    #[test]
    fn one_long_flag_one_arg_equals() {
        let args = args_as_ref(&["--foo=bar"]);
        let mut tokenizer = Tokenizer::new_with_iterator(args.into_iter());
        assert_eq!(tokenizer.next().unwrap(), Token::LongEq("foo", "bar"));
    }

    /// `foo= bar` should return an empty arg for `--foo` and treat `--bar` as
    /// a second arg.
    #[test]
    fn one_long_flag_missing_arg_equals() {
        let args = args_as_ref(&["--foo=", "bar"]);
        let mut tokenizer = Tokenizer::new_with_iterator(args.into_iter());
        assert_eq!(tokenizer.next().unwrap(), Token::LongEq("foo", ""));
        assert_eq!(tokenizer.next().unwrap(), Token::Arg(OsStr::new("bar")));
    }

    /// "--" should cause everything afterwards to be treated as an argument
    #[test]
    fn double_dash_stops_flag_parsing() {
        let args = args_as_ref(&["-a", "--foo", "bar", "--", "--baz", "-b", "hello"]);
        let mut tokenizer = Tokenizer::new_with_iterator(args.into_iter());
        assert_eq!(tokenizer.next().unwrap(), Token::Short('a'));
        assert_eq!(tokenizer.next().unwrap(), Token::Long("foo"));
        assert_eq!(tokenizer.next_arg().unwrap(), "bar");
        // After the '--' everything should be interpreted literally
        assert_eq!(tokenizer.next().unwrap(), Token::Arg(OsStr::new("--baz")));
        assert_eq!(tokenizer.next().unwrap(), Token::Arg(OsStr::new("-b")));
        assert_eq!(tokenizer.next().unwrap(), Token::Arg(OsStr::new("hello")));
    }
}
