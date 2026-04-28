//! Serialization helpers for GPUI types and blueprint persistence.
//!
//! This module provides custom serde serializers and deserializers for GPUI
//! geometric types (Point, Size, Hsla) that don't implement Serialize/Deserialize
//! by default. These helpers enable blueprint graphs to be saved and loaded.

use gpui::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// ============================================================================
// Point Serialization
// ============================================================================

pub mod point_serde {
    use super::*;

    #[derive(Serialize, Deserialize)]
    struct PointData {
        x: f32,
        y: f32,
    }

    pub fn serialize<S>(point: &Point<f32>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        PointData {
            x: point.x,
            y: point.y,
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Point<f32>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let data = PointData::deserialize(deserializer)?;
        Ok(Point::new(data.x, data.y))
    }
}

// ============================================================================
// Size Serialization
// ============================================================================

pub mod size_serde {
    use super::*;

    #[derive(Serialize, Deserialize)]
    struct SizeData {
        width: f32,
        height: f32,
    }

    pub fn serialize<S>(size: &Size<f32>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        SizeData {
            width: size.width,
            height: size.height,
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Size<f32>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let data = SizeData::deserialize(deserializer)?;
        Ok(Size::new(data.width, data.height))
    }
}

// ============================================================================
// HSLA Color Serialization
// ============================================================================

pub mod hsla_serde {
    use super::*;

    #[derive(Serialize, Deserialize)]
    struct HslaData {
        h: f32,
        s: f32,
        l: f32,
        a: f32,
    }

    pub fn serialize<S>(color: &Hsla, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        HslaData {
            h: color.h,
            s: color.s,
            l: color.l,
            a: color.a,
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Hsla, D::Error>
    where
        D: Deserializer<'de>,
    {
        let data = HslaData::deserialize(deserializer)?;
        Ok(Hsla {
            h: data.h,
            s: data.s,
            l: data.l,
            a: data.a,
        })
    }
}
