//! The Rust port of `src/Google.Protobuf/CodedOutputStream.ComputeSize.cs`.
//!
//! Google.Protobuf 3.29.0 (protobuf v29.0), Copyright 2008 Google Inc., BSD-3-Clause:
//! <https://github.com/protocolbuffers/protobuf/blob/v29.0/LICENSE>.
//!
//! The parameters are `i64` because the bridge carries every C# integer as one; the C# wrapper
//! only ever passes values of the original parameter type (`int`, `uint` or `long`).

#![forbid(unsafe_code)]

const LITTLE_ENDIAN_32_SIZE: i64 = 4;
const LITTLE_ENDIAN_64_SIZE: i64 = 8;

fn raw_varint64_size(value: u64) -> i64 {
    (1..10)
        .find(|&n| value & (u64::MAX << (7 * n)) == 0)
        .unwrap_or(10)
}

fn raw_varint32_size(value: u32) -> i64 {
    (1..5)
        .find(|&n| value & (u32::MAX << (7 * n)) == 0)
        .unwrap_or(5)
}

/// Port of `ComputeDoubleSize` (`src/Google.Protobuf/CodedOutputStream.ComputeSize.cs:30`).
pub fn compute_double_size(_value: f64) -> Result<i64, String> {
    Ok(LITTLE_ENDIAN_64_SIZE)
}

/// Port of `ComputeFloatSize` (`src/Google.Protobuf/CodedOutputStream.ComputeSize.cs:39`).
pub fn compute_float_size(_value: f64) -> Result<i64, String> {
    Ok(LITTLE_ENDIAN_32_SIZE)
}

/// Port of `ComputeInt64Size` (`src/Google.Protobuf/CodedOutputStream.ComputeSize.cs:57`).
pub fn compute_int64_size(value: i64) -> Result<i64, String> {
    Ok(raw_varint64_size(value as u64))
}

/// Port of `ComputeInt32Size` (`src/Google.Protobuf/CodedOutputStream.ComputeSize.cs:66`).
pub fn compute_int32_size(value: i64) -> Result<i64, String> {
    if value >= 0 {
        Ok(raw_varint32_size(value as u32))
    } else {
        Ok(10)
    }
}

/// Port of `ComputeFixed32Size` (`src/Google.Protobuf/CodedOutputStream.ComputeSize.cs:92`).
pub fn compute_fixed32_size(_value: i64) -> Result<i64, String> {
    Ok(LITTLE_ENDIAN_32_SIZE)
}

/// Port of `ComputeBoolSize` (`src/Google.Protobuf/CodedOutputStream.ComputeSize.cs:101`).
pub fn compute_bool_size(_value: bool) -> Result<i64, String> {
    Ok(1)
}

/// Port of `ComputeStringSize` (`src/Google.Protobuf/CodedOutputStream.ComputeSize.cs:110`).
pub fn compute_string_size(value: Option<String>) -> Result<i64, String> {
    let Some(value) = value else {
        return Err("System.ArgumentNullException: chars".into());
    };
    let bytes = value.len() as i64;
    Ok(raw_varint32_size(bytes as u32) + bytes)
}

/// Port of `ComputeUInt32Size` (`src/Google.Protobuf/CodedOutputStream.ComputeSize.cs:148`).
pub fn compute_u_int32_size(value: i64) -> Result<i64, String> {
    Ok(raw_varint32_size(value as u32))
}

/// Port of `ComputeEnumSize` (`src/Google.Protobuf/CodedOutputStream.ComputeSize.cs:158`).
pub fn compute_enum_size(value: i64) -> Result<i64, String> {
    compute_int32_size(value)
}

/// Port of `ComputeSFixed32Size` (`src/Google.Protobuf/CodedOutputStream.ComputeSize.cs:168`).
pub fn compute_s_fixed32_size(_value: i64) -> Result<i64, String> {
    Ok(LITTLE_ENDIAN_32_SIZE)
}

/// Port of `ComputeSFixed64Size` (`src/Google.Protobuf/CodedOutputStream.ComputeSize.cs:177`).
pub fn compute_s_fixed64_size(_value: i64) -> Result<i64, String> {
    Ok(LITTLE_ENDIAN_64_SIZE)
}

/// Port of `ComputeSInt32Size` (`src/Google.Protobuf/CodedOutputStream.ComputeSize.cs:186`).
pub fn compute_s_int32_size(value: i64) -> Result<i64, String> {
    let n = value as i32;
    Ok(raw_varint32_size(((n << 1) ^ (n >> 31)) as u32))
}

/// Port of `ComputeSInt64Size` (`src/Google.Protobuf/CodedOutputStream.ComputeSize.cs:195`).
pub fn compute_s_int64_size(value: i64) -> Result<i64, String> {
    Ok(raw_varint64_size(((value << 1) ^ (value >> 63)) as u64))
}

/// Port of `ComputeLengthSize` (`src/Google.Protobuf/CodedOutputStream.ComputeSize.cs:204`).
pub fn compute_length_size(length: i64) -> Result<i64, String> {
    Ok(raw_varint32_size(length as i32 as u32))
}

/// Port of `ComputeRawVarint32Size` (`src/Google.Protobuf/CodedOutputStream.ComputeSize.cs:212`).
pub fn compute_raw_varint32_size(value: i64) -> Result<i64, String> {
    Ok(raw_varint32_size(value as u32))
}

/// Port of `ComputeTagSize` (`src/Google.Protobuf/CodedOutputStream.ComputeSize.cs:280`).
pub fn compute_tag_size(field_number: i64) -> Result<i64, String> {
    Ok(raw_varint32_size(((field_number as i32) << 3) as u32))
}
