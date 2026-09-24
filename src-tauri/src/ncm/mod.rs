//! ncm 解密核心：密钥常量、AES/流式解密、内嵌元数据解析。
//! 算法忠实移植自 taurusxin/ncmdump 的 src/ncmcrypt.cpp（UTF-8 全兼容版本）。

pub mod crypt;
pub mod keys;
pub mod metadata;

pub use crypt::{AudioFormat, NcmFile};
pub use metadata::NcmMetadata;
