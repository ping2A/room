//! Typed 32-bit identifiers for definitions and map geometry.

macro_rules! typed_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
        pub struct $name(pub u32);

        impl $name {
            /// Raw numeric value.
            #[must_use]
            pub const fn raw(self) -> u32 {
                self.0
            }

            /// Construct from a raw index.
            #[must_use]
            pub const fn from_raw(v: u32) -> Self {
                Self(v)
            }
        }
    };
}

typed_id!(
    /// Actor class identifier. Equivalent of GZDoom `PClass` index / `ClassId`.
    ClassId
);
typed_id!(
    /// Index into the flat state table.
    StateId
);
typed_id!(
    /// Texture registry identifier.
    TextureId
);
typed_id!(
    /// Logical sound identifier from SNDINFO.
    SoundId
);
typed_id!(
    /// Lump identifier in a mounted VFS.
    LumpId
);
typed_id!(
    /// Map sector index.
    SectorId
);
typed_id!(
    /// Map linedef index.
    LineId
);
typed_id!(
    /// Map sidedef index.
    SideId
);
typed_id!(
    /// Map vertex index.
    VertexId
);
