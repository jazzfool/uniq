use std::marker::PhantomData;

/// Marker denoting an immutable parameter which will appear in the `Listener` method signatures and handlers as `&T`.
pub struct Read<T: ?Sized + 'static>(PhantomData<T>);
/// Marker denoting a mutable parameter which will appear in the `Listener` method signatures and handlers as `&mut T`.
pub struct Write<T: ?Sized + 'static>(PhantomData<T>);

// We need to "package" references into raw pointers in order to evade dropck.

// Without this, it becomes near impossible to work with the lifetime of the references
// without running into HRTB ICEs (OutputTypeParameterMismatch).

// Once lazy normalization is implemented, this should be revisited.

/// Parameter type which can be packed into a copyable non-reference type.
pub trait Packable {
    /// The copyable non-reference "packed" type.
    type Packed: Copy;
}

/// Parameter type which can be unpacked from a copyable non-reference type into a reference with a specified lifetime.
pub trait Unpackable<'a>: Packable {
    /// The reference type with the specified lifetime.
    type Unpacked: 'a;

    /// Packs an unpacked reference into a packed non-reference.
    fn pack(unpacked: Self::Unpacked) -> Self::Packed;
    /// Unpacks an non-reference type into an unpacked reference.
    unsafe fn unpack(packed: Self::Packed) -> Self::Unpacked;
}

impl Packable for () {
    type Packed = ();
}

impl<'a> Unpackable<'a> for () {
    type Unpacked = ();

    fn pack(_: ()) {}
    unsafe fn unpack(_: ()) {}
}

impl<T: ?Sized + 'static> Packable for Read<T> {
    type Packed = *const T;
}

impl<'a, T: ?Sized + 'static> Unpackable<'a> for Read<T> {
    type Unpacked = &'a T;

    #[inline]
    fn pack(unpacked: Self::Unpacked) -> Self::Packed {
        unpacked
    }

    #[inline]
    unsafe fn unpack(packed: Self::Packed) -> Self::Unpacked {
        &*packed
    }
}

impl<T: ?Sized + 'static> Packable for Write<T> {
    type Packed = *mut T;
}

impl<'a, T: ?Sized + 'static> Unpackable<'a> for Write<T> {
    type Unpacked = &'a mut T;

    #[inline]
    fn pack(unpacked: Self::Unpacked) -> Self::Packed {
        unpacked
    }

    #[inline]
    unsafe fn unpack(packed: Self::Packed) -> Self::Unpacked {
        &mut *packed
    }
}

macro_rules! impl_packaging {
    ($($x:ident = $i:tt),*) => {
        impl<$($x: Packable),*> Packable for ($($x,)*) {
            type Packed = ($(<$x as Packable>::Packed),*);
        }

        impl<'a, $($x: Unpackable<'a>),*> Unpackable<'a> for ($($x,)*) {
            type Unpacked = ($(<$x as Unpackable<'a>>::Unpacked),*);

            fn pack(unpacked: Self::Unpacked) -> Self::Packed {
                ($($x::pack(unpacked.$i)),*)
            }

            unsafe fn unpack(packed: Self::Packed) -> Self::Unpacked {
                ($($x::unpack(packed.$i)),*)
            }
        }
    };
}

// Supports up to 10-size tuples.

impl_packaging!(A = 0, B = 1);
impl_packaging!(A = 0, B = 1, C = 2);
impl_packaging!(A = 0, B = 1, C = 2, D = 3);
impl_packaging!(A = 0, B = 1, C = 2, D = 3, E = 4);
impl_packaging!(A = 0, B = 1, C = 2, D = 3, E = 4, F = 5);
impl_packaging!(A = 0, B = 1, C = 2, D = 3, E = 4, F = 5, G = 6);
impl_packaging!(A = 0, B = 1, C = 2, D = 3, E = 4, F = 5, G = 6, H = 7);
impl_packaging!(
    A = 0,
    B = 1,
    C = 2,
    D = 3,
    E = 4,
    F = 5,
    G = 6,
    H = 7,
    I = 8
);
impl_packaging!(
    A = 0,
    B = 1,
    C = 2,
    D = 3,
    E = 4,
    F = 5,
    G = 6,
    H = 7,
    I = 8,
    J = 9
);
