use super::*;

pub fn add_window(scope: &mut HandleScope<'_>, context: &Local<'_, Context>) {
    let window_name = v8::String::new(scope, "window").unwrap();
    let window_value = v8::Object::new(scope);

    let global = context.global(scope);
    global
        .set(scope, window_name.into(), window_value.into())
        .unwrap();
}
