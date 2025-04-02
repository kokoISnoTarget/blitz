#[macro_export]
macro_rules! fast_str {
    ($str:expr_2021) => {{
        const C: v8::OneByteConst = v8::String::create_external_onebyte_const($str.as_bytes());
        C
    }};
}
pub trait OneByteConstExt {
    fn to_v8<'a>(&'static self, scope: &mut v8::HandleScope<'a>) -> v8::Local<'a, v8::String>;
}
impl OneByteConstExt for v8::OneByteConst {
    fn to_v8<'a>(&'static self, scope: &mut v8::HandleScope<'a>) -> v8::Local<'a, v8::String> {
        v8::String::new_from_onebyte_const(scope, self).unwrap()
    }
}

pub const fn todo<T>() -> T {
    todo!()
}
