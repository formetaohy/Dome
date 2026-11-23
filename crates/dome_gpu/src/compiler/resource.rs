use std::marker::PhantomData;
use std::ops::{Add, Index, IndexMut, Mul, Sub};

pub struct Read<T>(PhantomData<T>);
pub struct ReadWrite<T>(PhantomData<T>);

impl<T> Default for Read<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T> Default for ReadWrite<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}
pub struct In<T>(PhantomData<T>);
pub struct Out<T>(PhantomData<T>);
pub struct Position;

pub struct Workgroup<T, const N: usize>(PhantomData<T>);

impl<T, const N: usize> Workgroup<T, N> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T, const N: usize> Default for Workgroup<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> Index<u32> for Workgroup<T, N> {
    type Output = T;

    fn index(&self, _: u32) -> &Self::Output {
        unreachable!("device workgroup memory exists only on the device")
    }
}

impl<T, const N: usize> IndexMut<u32> for Workgroup<T, N> {
    fn index_mut(&mut self, _: u32) -> &mut Self::Output {
        unreachable!("device workgroup memory exists only on the device")
    }
}

pub struct AtomicU32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Texture2D;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sampler;

pub struct TextureTable;

pub struct SamplerTable;

impl Default for Texture2D {
    fn default() -> Self {
        Self
    }
}

impl Default for Sampler {
    fn default() -> Self {
        Self
    }
}

impl Default for TextureTable {
    fn default() -> Self {
        Self
    }
}

impl Default for SamplerTable {
    fn default() -> Self {
        Self
    }
}

impl Index<u32> for TextureTable {
    type Output = Texture2D;

    fn index(&self, _: u32) -> &Self::Output {
        unreachable!("a device texture table exists only on the device")
    }
}

impl Index<u32> for SamplerTable {
    type Output = Sampler;

    fn index(&self, _: u32) -> &Self::Output {
        unreachable!("a device sampler table exists only on the device")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Uvec3 {
    pub x: u32,
    pub y: u32,
    pub z: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uvec4 {
    pub x: u32,
    pub y: u32,
    pub z: u32,
    pub w: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fvec2 {
    pub x: f32,
    pub y: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Fvec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Add for Fvec4 {
    type Output = Self;

    fn add(self, _other: Self) -> Self {
        unreachable!("a device vector exists only on the device")
    }
}

impl Sub for Fvec4 {
    type Output = Self;

    fn sub(self, _other: Self) -> Self {
        unreachable!("a device vector exists only on the device")
    }
}

impl Mul<f32> for Fvec4 {
    type Output = Self;

    fn mul(self, _scalar: f32) -> Self {
        unreachable!("a device vector exists only on the device")
    }
}

impl Mul<Fvec4> for f32 {
    type Output = Fvec4;

    fn mul(self, _vector: Fvec4) -> Fvec4 {
        unreachable!("a device vector exists only on the device")
    }
}

impl Mul for Fvec4 {
    type Output = Self;

    fn mul(self, _other: Self) -> Self {
        unreachable!("a device vector exists only on the device")
    }
}

impl<T> Index<u32> for Read<T> {
    type Output = T;

    fn index(&self, _: u32) -> &Self::Output {
        unreachable!("a kernel resource exists only on the device")
    }
}

impl<T> Index<u32> for ReadWrite<T> {
    type Output = T;

    fn index(&self, _: u32) -> &Self::Output {
        unreachable!("a kernel resource exists only on the device")
    }
}

impl<T> IndexMut<u32> for ReadWrite<T> {
    fn index_mut(&mut self, _: u32) -> &mut Self::Output {
        unreachable!("a kernel resource exists only on the device")
    }
}
