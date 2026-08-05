use crate::bash_api::{c_char, c_int, WORD_LIST};

extern "C" {
    pub(crate) fn internal_getopt(list: *mut WORD_LIST, optstring: *const c_char) -> c_int;
    pub(crate) fn reset_internal_getopt();
    pub(crate) static mut list_optarg: *mut c_char;
    pub(crate) static mut loptend: *mut WORD_LIST;
}

#[macro_export]
macro_rules! bash_getopt {
    (
        $list:expr,
        $help_fn:expr,
        [ $( $flag:ident ),* $(,)? ],
        [ $( $opt:ident ),* $(,)? ] $(,)?
    ) => {{
        const OPTSTRING: &[u8] = concat!(
            $( stringify!($flag), )*
            $( stringify!($opt), ":" ),*
        ).as_bytes();
        struct ParsedOpts {
            $( pub $flag: bool, )*
            $( pub $opt: Option<*mut std::os::raw::c_char>, )*
        }
        #[allow(unused_mut)]
        let mut parsed = ParsedOpts {
            $( $flag: false, )*
            $( $opt: None, )*
        };
        $crate::bash_getopt::reset_internal_getopt();
        let list = $list;
        loop {
            let c = unsafe {
                $crate::bash_getopt::internal_getopt(list, OPTSTRING.as_ptr().cast())
            };
            match c {
                -1 => break,
                c if c == b'h' as std::os::raw::c_int => { $help_fn(); return 0; }
                -99 => { $help_fn(); return 2; },
                $(
                    c if c == stringify!($flag).as_bytes()[0] as std::os::raw::c_int => {
                        parsed.$flag = true;
                    }
                )*
                $(
                    c if c == stringify!($opt).as_bytes()[0] as std::os::raw::c_int => {
                        parsed.$opt = Some(unsafe { $crate::bash_getopt::list_optarg.cast() });
                    }
                )*
                _ => return EX_USAGE,
            }
        };
        (parsed, $crate::bash_getopt::loptend)
    }};
}
