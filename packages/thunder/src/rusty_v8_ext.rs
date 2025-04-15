use v8::{CallbackScope, Context, HandleScope, Local, Module, Object};

#[allow(private_bounds)]
pub trait HostInitializeImportMetaObjectCallback:
    UnitType + for<'s> FnOnce(&mut HandleScope<'s>, Local<'s, Module>, Local<'s, Object>)
{
    fn to_c_fn(
        self,
    ) -> for<'s> unsafe extern "C" fn(Local<'s, Context>, Local<'s, Module>, Local<'s, Object>);
}
impl<F> HostInitializeImportMetaObjectCallback for F
where
    F: UnitType + for<'s> FnOnce(&mut HandleScope<'s>, Local<'s, Module>, Local<'s, Object>),
{
    fn to_c_fn(
        self,
    ) -> for<'s> unsafe extern "C" fn(Local<'s, Context>, Local<'s, Module>, Local<'s, Object>)
    {
        unsafe extern "C" fn adapter<F: HostInitializeImportMetaObjectCallback>(
            context: Local<'_, Context>,
            module: Local<'_, Module>,
            meta: Local<'_, Object>,
        ) {
            let scope = &mut unsafe { CallbackScope::new(context) };
            (F::get())(scope, module, meta);
        }
        adapter::<F>
    }
}

/// From rust_v8 internals https://github.com/denoland/rusty_v8/blob/3f237d58b7ffddf346a1f32d52fe6e4f99d421b9/src/support.rs#L563C1-L573C48
trait UnitType
where
    Self: Copy + Sized,
{
    #[inline(always)]

    fn get() -> Self {
        UnitValue::<Self>::get()
    }
}

impl<T> UnitType for T where T: Copy + Sized {}

#[derive(Copy, Clone, Debug)]

/// From rust_v8 internals https://github.com/denoland/rusty_v8/blob/3f237d58b7ffddf346a1f32d52fe6e4f99d421b9/src/support.rs#L575C1-L608C1
struct UnitValue<T>(std::marker::PhantomData<T>)
where
    Self: Sized;

impl<T> UnitValue<T>
where
    Self: Copy + Sized,
{
    const SELF: Self = Self::new_checked();

    const fn new_checked() -> Self {
        // Statically assert that T is indeed a unit type.

        let size_must_be_0 = size_of::<T>();

        let s = Self(std::marker::PhantomData::<T>);

        [s][size_must_be_0]
    }

    #[inline(always)]

    fn get_checked(self) -> T {
        // This run-time check serves just as a backup for the compile-time

        // check when Self::SELF is initialized.

        assert_eq!(size_of::<T>(), 0);

        unsafe { std::mem::MaybeUninit::<T>::zeroed().assume_init() }
    }

    #[inline(always)]

    pub fn get() -> T {
        // Accessing the Self::SELF is necessary to make the compile-time type check

        // work.

        Self::SELF.get_checked()
    }
}
