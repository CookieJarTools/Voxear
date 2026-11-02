//! 2D physics on Box2D v3, which `boxdd-sys` builds from its C source with
//! ready-made bindings, behind a small safe layer of Voxear's own
//! ([`world`]). Objs move in the XY plane and turn about Z, as in Unity's
//! 2D physics. Y is up, and units are metres, kilograms and seconds.

pub mod scene;
pub mod world;

pub use world::{Body2D, BodyDesc2D, Contact2D, Motion2D, PhysicsWorld2D, Push2D, RayHit2D, Shape2D};
