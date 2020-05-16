use serde::ser::{Serialize, Serializer};
use serde::de::{self, Deserialize, Deserializer, Visitor};

use std::fmt;
use std::marker::PhantomData;

use crate::unsync::OnceCell;
use crate::sync::OnceCell as SyncOnceCell;

impl<T: Serialize> Serialize for OnceCell<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.get() {
            Some(val) => serializer.serialize_some(val),
            None => serializer.serialize_none()
        }
    }
}


impl<T: Serialize> Serialize for SyncOnceCell<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.get() {
            Some(val) => serializer.serialize_some(val),
            None => serializer.serialize_none()
        }
    }
}

struct OnceCellVisitor<T>(PhantomData<*const T>);
impl<'de, T: Deserialize<'de>> Visitor<'de> for OnceCellVisitor<T> {
    type Value = OnceCell<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an OnceCell")
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        Ok(OnceCell::from(T::deserialize(deserializer)?))
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(OnceCell::new())
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for OnceCell<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_option(OnceCellVisitor(PhantomData))
    }
}


struct SyncOnceCellVisitor<T>(PhantomData<*const T>);
impl<'de, T: Deserialize<'de>> Visitor<'de> for SyncOnceCellVisitor<T> {
    type Value = SyncOnceCell<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an OnceCell")
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        Ok(SyncOnceCell::from(T::deserialize(deserializer)?))
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(SyncOnceCell::new())
    }
}


impl<'de, T: Deserialize<'de>> Deserialize<'de> for SyncOnceCell<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_option(SyncOnceCellVisitor(PhantomData))
    }
}