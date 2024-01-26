use crate::error::{Error, Result};
use derp::{Der, Tag};
use ed25519_dalek::pkcs8::spki::der::pem;
use hex_fmt::HexFmt;
use rand::Rng;
use std::{fmt, path::Path};
use tokio::fs;

const ED25519_OBJECT_IDENTIFIER: [u8; 3] = [43, 101, 112];
const SECP256K1_OBJECT_IDENTIFIER: [u8; 5] = [43, 129, 4, 0, 10];

#[derive(Debug, Clone)]
pub enum PublicKey {
    /// Ed25519 public key.
    Ed25519(ed25519_dalek::VerifyingKey),
    /// secp256k1 public key.
    Secp256k1(k256::ecdsa::VerifyingKey),
}

#[derive(Debug)]
pub enum SecretKey {
    /// Ed25519 public key.
    Ed25519(ed25519_dalek::SigningKey),
    /// secp256k1 public key.
    Secp256k1(k256::ecdsa::SigningKey),
}

pub fn generate_pair(rng: &mut impl Rng) -> (PublicKey, SecretKey) {
    let bytes = rng.gen();

    if rng.gen() {
        let secret_key = ed25519_dalek::SigningKey::from_bytes(&bytes);
        let public_key = secret_key.verifying_key();

        (
            PublicKey::Ed25519(public_key),
            SecretKey::Ed25519(secret_key),
        )
    } else {
        let secret_key =
            k256::ecdsa::SigningKey::from_slice(&bytes).expect("The bytes to be valid");
        let public_key = secret_key.verifying_key().clone();

        (
            PublicKey::Secp256k1(public_key),
            SecretKey::Secp256k1(secret_key),
        )
    }
}

impl SecretKey {
    fn pem(&self) -> Result<String> {
        let label = match self {
            Self::Ed25519(_) => "PRIVATE KEY",
            Self::Secp256k1(_) => "EC PRIVATE KEY",
        };

        let result = pem::encode_string(label, pem::LineEnding::CRLF, &self.der()?)?;

        Ok(result)
    }

    fn der(&self) -> Result<Vec<u8>> {
        match self {
            Self::Ed25519(secret_key) => {
                // See https://tools.ietf.org/html/rfc8410#section-10.3
                let mut key_bytes = vec![];
                let mut der = Der::new(&mut key_bytes);
                der.octet_string(&secret_key.to_bytes())?;

                let mut encoded = vec![];
                der = Der::new(&mut encoded);
                der.sequence(|der| {
                    der.integer(&[0])?;
                    der.sequence(|der| der.oid(&ED25519_OBJECT_IDENTIFIER))?;
                    der.octet_string(&key_bytes)
                })?;
                Ok(encoded)
            }
            Self::Secp256k1(secret_key) => {
                // See https://www.secg.org/sec1-v2.pdf#subsection.C.4
                let mut oid_bytes = vec![];
                let mut der = Der::new(&mut oid_bytes);
                der.oid(&SECP256K1_OBJECT_IDENTIFIER)?;

                let mut encoded = vec![];
                der = Der::new(&mut encoded);
                der.sequence(|der| {
                    der.integer(&[1])?;
                    der.octet_string(secret_key.to_bytes().as_slice())?;
                    der.element(Tag::ContextSpecificConstructed0, &oid_bytes)
                })?;
                Ok(encoded)
            }
        }
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PublicKey::Ed25519(key) => write!(f, "01{}", HexFmt(key.as_bytes())),
            PublicKey::Secp256k1(key) => write!(f, "02{}", HexFmt(key.to_sec1_bytes())),
        }
    }
}

pub async fn write_pem(secret_key: &SecretKey, path: impl AsRef<Path>) -> Result<()> {
    let pem_string = secret_key.pem()?;
    let path = path.as_ref();

    fs::write(&path, pem_string)
        .await
        .map_err(|io_err| Error::FileOperation {
            description: format!("cannot write the pem file {path:?}"),
            io_err,
        })?;

    Ok(())
}
