use super::*;
pub fn add_console(scope: &mut HandleScope<'_>, context: &Local<'_, Context>) {
    let console = Object::new(scope);

    for &mode in LOG_LEVELS {
        let name = v8::String::new(scope, &mode.as_ref()).unwrap();
        let data = Integer::new_from_unsigned(scope, mode as u32);

        let func = v8::FunctionBuilder::<'_, Function>::new(
            |scope: &mut HandleScope<'_>,
             args: FunctionCallbackArguments<'_>,
             mut retval: ReturnValue<'_>| {
                let mode = args.data().uint32_value(scope).unwrap();
                let mode = match mode {
                    0 => LoggerMode::Log,
                    1 => LoggerMode::Debug,
                    2 => LoggerMode::Info,
                    3 => LoggerMode::Warn,
                    4 => LoggerMode::Error,
                    _ => LoggerMode::Log,
                };
                logger(scope, args, mode);
                retval.set_undefined();
            },
        )
        .data(data.into())
        .build(scope)
        .unwrap();
        console.set(scope, name.into(), func.into());
    }

    let global = context.global(scope);
    let name = v8::String::new(scope, "console").unwrap();
    global.set(scope, name.into(), console.into());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoggerMode {
    Log,
    Debug,
    Info,
    Warn,
    Error,
}
impl AsRef<str> for LoggerMode {
    fn as_ref(&self) -> &str {
        match self {
            LoggerMode::Log => "log",
            LoggerMode::Debug => "debug",
            LoggerMode::Info => "info",
            LoggerMode::Warn => "warn",
            LoggerMode::Error => "error",
        }
    }
}
const LOG_LEVELS: &[LoggerMode] = &[
    LoggerMode::Log,
    LoggerMode::Debug,
    LoggerMode::Info,
    LoggerMode::Warn,
    LoggerMode::Error,
];

// https://console.spec.whatwg.org/#logger
fn logger(scope: &mut HandleScope<'_>, args: FunctionCallbackArguments<'_>, _mode: LoggerMode) {
    let len = args.length();
    if len == 0 {
        return;
    }

    let _first = args.get(0);

    // TODO: Maybe implement all
    let mut out = String::new();
    for i in 0..len {
        let arg = args.get(i);
        out.push_str(&arg.to_rust_string_lossy(scope));
    }
    println!("{}", out);
}
