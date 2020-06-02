use std::ffi::c_void;
use std::fmt;
use std::ops;

use crate::config::VideoMeta;

use chrono::{DateTime, Utc};
use opencv::core::Mat_AUTO_STEP;
use opencv::prelude::{Mat, MatTrait};
use podo_core_driver::RuntimeError;
use serde::{
    de::{self, MapAccess, SeqAccess, Visitor},
    ser::SerializeStruct,
    Deserialize, Deserializer, Serialize, Serializer,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct Frame {
    pub image: Image,
    pub meta: VideoMeta,
    pub timestamp: DateTime<Utc>,

    pub(crate) count: usize,
}

impl Frame {
    pub fn new(meta: VideoMeta) -> Result<Self, RuntimeError> {
        Ok(Self {
            image: Image::try_default()?,
            meta,
            timestamp: Utc::now(),
            count: 0,
        })
    }
}

#[derive(Debug)]
pub struct Image {
    inner: Mat,
    data: Option<Vec<u8>>,
}

impl Image {
    pub(crate) fn try_default() -> Result<Self, RuntimeError> {
        Ok(Self {
            inner: Mat::default()?,
            data: None,
        })
    }

    fn from_bytes(rows: i32, cols: i32, typ: i32, mut data: Vec<u8>) -> Result<Self, RuntimeError> {
        let ptr = data.as_mut_ptr() as *mut c_void;
        let mat = unsafe { Mat::new_rows_cols_with_data(rows, cols, typ, ptr, Mat_AUTO_STEP)? };

        Ok(Self {
            inner: mat,
            data: Some(data),
        })
    }
}

impl From<Mat> for Image {
    fn from(mat: Mat) -> Self {
        Self {
            inner: mat,
            data: None,
        }
    }
}

impl ops::Deref for Image {
    type Target = Mat;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl ops::DerefMut for Image {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl Serialize for Image {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let rows = self.inner.rows() as usize;
        let cols = self.inner.cols() as usize;
        let typ = self.inner.typ().unwrap();
        let elem_size = self.inner.elem_size().unwrap() as usize;

        let len = rows * cols * elem_size;

        let mut state = serializer.serialize_struct("image", 4)?;
        state.serialize_field("rows", &(rows as i32))?;
        state.serialize_field("cols", &(cols as i32))?;
        state.serialize_field("typ", &typ)?;

        let slice = unsafe { std::slice::from_raw_parts(self.inner.ptr(0).unwrap(), len as usize) };
        state.serialize_field("data", slice)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for Image {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Rows,
            Cols,
            Typ,
            Data,
        };

        struct ImageVisitor;

        impl<'de> Visitor<'de> for ImageVisitor {
            type Value = Image;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct Image")
            }

            fn visit_seq<V>(self, mut seq: V) -> Result<Image, V::Error>
            where
                V: SeqAccess<'de>,
            {
                let rows = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let cols = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                let typ = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(2, &self))?;
                let data = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(3, &self))?;
                Ok(Image::from_bytes(rows, cols, typ, data).unwrap())
            }

            fn visit_map<V>(self, mut map: V) -> Result<Image, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut rows = None;
                let mut cols = None;
                let mut typ = None;
                let mut data = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Rows => {
                            if rows.is_some() {
                                return Err(de::Error::duplicate_field("rows"));
                            }
                            rows = Some(map.next_value()?);
                        }
                        Field::Cols => {
                            if cols.is_some() {
                                return Err(de::Error::duplicate_field("cols"));
                            }
                            cols = Some(map.next_value()?);
                        }
                        Field::Typ => {
                            if typ.is_some() {
                                return Err(de::Error::duplicate_field("typ"));
                            }
                            typ = Some(map.next_value()?);
                        }
                        Field::Data => {
                            if data.is_some() {
                                return Err(de::Error::duplicate_field("data"));
                            }
                            data = Some(map.next_value()?);
                        }
                    }
                }
                let rows = rows.ok_or_else(|| de::Error::missing_field("rows"))?;
                let cols = cols.ok_or_else(|| de::Error::missing_field("cols"))?;
                let typ = typ.ok_or_else(|| de::Error::missing_field("typ"))?;
                let data = data.ok_or_else(|| de::Error::missing_field("data"))?;
                Ok(Image::from_bytes(rows, cols, typ, data).unwrap())
            }
        }

        const FIELDS: &[&str] = &["rows", "cols", "typ", "data"];
        deserializer.deserialize_struct("image", FIELDS, ImageVisitor)
    }
}

#[test]
fn serde_support() {
    let mut mat = unsafe { Mat::new_rows_cols(42, 37, opencv::core::CV_64FC1).unwrap() };
    *mat.at_2d_mut::<f64>(11, 22).unwrap() = 42.0;

    let image = Image::from(mat);

    let image_byte = bincode::serialize(&image).unwrap();
    let image_clone: Image = bincode::deserialize(&image_byte).unwrap();

    assert_eq!(image.inner.rows(), image_clone.rows());
    assert_eq!(image.inner.cols(), image_clone.cols());
    assert_eq!(image.inner.typ().unwrap(), image_clone.typ().unwrap());

    assert_eq!(*image_clone.inner.at_2d::<f64>(11, 22).unwrap(), 42.0);
    assert_eq!(*image_clone.inner.at_2d::<f64>(22, 11).unwrap(), 0.0);
}
