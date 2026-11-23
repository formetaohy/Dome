#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldType {
    U32,
    I32,
    F32,
    U32x4,
    F32x4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldLayout {
    pub name: &'static str,
    pub offset: u32,
    pub ty: FieldType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordLayout {
    pub name: &'static str,
    pub size: u32,
    pub fields: &'static [FieldLayout],
}

pub trait Record {
    const LAYOUT: RecordLayout;
}

#[macro_export]
macro_rules! record {
    ($record:ident, $layout:ident, $name:literal {
        $($field:ident: $ty:ty => $kind:ident),+ $(,)?
    }) => {
        #[repr(C)]
        #[derive(Clone, Copy, Debug, PartialEq, ::bytemuck::Pod, ::bytemuck::Zeroable)]
        pub struct $record {
            $(pub $field: $ty,)+
        }

        pub const $layout: $crate::layout::RecordLayout = $crate::layout::RecordLayout {
            name: $name,
            size: ::core::mem::size_of::<$record>() as u32,
            fields: &[$( $crate::layout::FieldLayout {
                name: ::core::stringify!($field),
                offset: ::core::mem::offset_of!($record, $field) as u32,
                ty: $crate::layout::FieldType::$kind,
            },)+],
        };

        impl $crate::layout::Record for $record {
            const LAYOUT: $crate::layout::RecordLayout = $layout;
        }
    };
}
