//! This crate implements bcrypt_pbkdf, a custom derivative of PBKDF2 used in
//! OpenSSH.

use blowfish::Blowfish;
use byteorder::{ByteOrder, BE, LE};
use crypto_mac::{
    generic_array::{typenum::U32, GenericArray},
    Mac, MacResult,
};
use pbkdf2::pbkdf2;
use sha2::{Digest, Sha512};

const BHASH_WORDS: usize = 8;
const BHASH_OUTPUT_SIZE: usize = BHASH_WORDS * 4;
const BHASH_SEED: &[u8; BHASH_OUTPUT_SIZE] = b"OxychromaticBlowfishSwatDynamite";

fn bhash(sha2_pass: &[u8], sha2_salt: &[u8]) -> [u8; BHASH_OUTPUT_SIZE] {
    assert_eq!(sha2_pass.len(), <Sha512 as Digest>::output_size());
    assert_eq!(sha2_salt.len(), <Sha512 as Digest>::output_size());

    let mut blowfish = Blowfish::bc_init_state();

    blowfish.salted_expand_key(sha2_salt, sha2_pass);
    for _ in 0..64 {
        blowfish.bc_expand_key(sha2_salt);
        blowfish.bc_expand_key(sha2_pass);
    }

    let mut cdata = [0u32; BHASH_WORDS];
    for i in 0..BHASH_WORDS {
        cdata[i] = BE::read_u32(&BHASH_SEED[i * 4..(i + 1) * 4]);
    }

    for _ in 0..64 {
        for i in (0..BHASH_WORDS).step_by(2) {
            let (l, r) = blowfish.bc_encrypt(cdata[i], cdata[i + 1]);
            cdata[i] = l;
            cdata[i + 1] = r;
        }
    }

    let mut output = [0u8; BHASH_OUTPUT_SIZE];
    for i in 0..BHASH_WORDS {
        LE::write_u32(&mut output[i * 4..(i + 1) * 4], cdata[i]);
    }

    output
}

#[derive(Clone)]
struct Bhash {
    sha2_pass: GenericArray<u8, <Sha512 as Digest>::OutputSize>,
    salt: Sha512,
}

impl Mac for Bhash {
    type OutputSize = U32;
    type KeySize = <Sha512 as Digest>::OutputSize;

    fn new(key: &GenericArray<u8, Self::KeySize>) -> Self {
        Bhash {
            sha2_pass: *key,
            salt: Sha512::default(),
        }
    }

    fn input(&mut self, data: &[u8]) {
        self.salt.input(data);
    }

    fn reset(&mut self) {
        self.salt.reset();
    }

    fn result(self) -> MacResult<Self::OutputSize> {
        let output = bhash(&self.sha2_pass, &self.salt.result());
        MacResult::new(GenericArray::clone_from_slice(&output[..]))
    }
}

pub fn bcrypt_pbkdf(passphrase: &str, salt: &[u8], rounds: u32, output: &mut [u8]) {
    // Allocate a Vec large enough to hold the output we require.
    let stride = (output.len() + BHASH_OUTPUT_SIZE - 1) / BHASH_OUTPUT_SIZE;
    let mut generated = vec![0; stride * BHASH_OUTPUT_SIZE];

    // Run the regular PBKDF2 algorithm with bhash as the MAC.
    pbkdf2::<Bhash>(
        &Sha512::digest(passphrase.as_bytes()),
        salt,
        rounds as usize,
        &mut generated,
    );

    // Apply the bcrypt_pbkdf non-linear transformation on the output.
    for (i, out_byte) in output.iter_mut().enumerate() {
        let chunk_num = i % stride;
        let chunk_index = i / stride;
        *out_byte = generated[chunk_num * BHASH_OUTPUT_SIZE + chunk_index];
    }
}

#[cfg(test)]
mod test {
    use super::{bcrypt_pbkdf, bhash};

    #[test]
    fn test_bhash() {
        struct Test {
            hpass: [u8; 64],
            hsalt: [u8; 64],
            out: [u8; 32],
        }

        let tests = vec![
            Test {
                hpass: [
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                ],
                hsalt: [
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                ],
                out: [
                    0x46, 0x02, 0x86, 0xe9, 0x72, 0xfa, 0x83, 0x3f, 0x8b, 0x12, 0x83, 0xad, 0x8f,
                    0xa9, 0x19, 0xfa, 0x29, 0xbd, 0xe2, 0x0e, 0x23, 0x32, 0x9e, 0x77, 0x4d, 0x84,
                    0x22, 0xba, 0xc0, 0xa7, 0x92, 0x6c,
                ],
            },
            Test {
                hpass: [
                    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c,
                    0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19,
                    0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26,
                    0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f, 0x30, 0x31, 0x32, 0x33,
                    0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f,
                ],
                hsalt: [
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                ],
                out: [
                    0xb0, 0xb2, 0x29, 0xdb, 0xc6, 0xba, 0xde, 0xf0, 0xe1, 0xda, 0x25, 0x27, 0x47,
                    0x4a, 0x8b, 0x28, 0x88, 0x8f, 0x8b, 0x06, 0x14, 0x76, 0xfe, 0x80, 0xc3, 0x22,
                    0x56, 0xe1, 0x14, 0x2d, 0xd0, 0x0d,
                ],
            },
            Test {
                hpass: [
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                ],
                hsalt: [
                    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c,
                    0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19,
                    0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26,
                    0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f, 0x30, 0x31, 0x32, 0x33,
                    0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f,
                ],
                out: [
                    0xb6, 0x2b, 0x4e, 0x36, 0x7d, 0x31, 0x57, 0xf5, 0xc3, 0x1e, 0x4d, 0x2c, 0xba,
                    0xfb, 0x29, 0x31, 0x49, 0x4d, 0x9d, 0x3b, 0xdd, 0x17, 0x1d, 0x55, 0xcf, 0x79,
                    0x9f, 0xa4, 0x41, 0x60, 0x42, 0xe2,
                ],
            },
            Test {
                hpass: [
                    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c,
                    0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19,
                    0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26,
                    0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f, 0x30, 0x31, 0x32, 0x33,
                    0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f,
                ],
                hsalt: [
                    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c,
                    0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19,
                    0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26,
                    0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f, 0x30, 0x31, 0x32, 0x33,
                    0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f,
                ],
                out: [
                    0xc6, 0xa9, 0x5f, 0xe6, 0x41, 0x31, 0x15, 0xfb, 0x57, 0xe9, 0x9f, 0x75, 0x74,
                    0x98, 0xe8, 0x5d, 0xa3, 0xc6, 0xe1, 0xdf, 0x0c, 0x3c, 0x93, 0xaa, 0x97, 0x5c,
                    0x54, 0x8a, 0x34, 0x43, 0x26, 0xf8,
                ],
            },
        ];

        for t in tests.iter() {
            let out = bhash(&t.hpass, &t.hsalt);
            assert_eq!(out, t.out);
        }
    }

    #[test]
    fn test_openbsd_vectors() {
        struct Test {
            password: &'static str,
            salt: Vec<u8>,
            rounds: u32,
            out: Vec<u8>,
        }

        let tests = vec!(
            Test{
                password: "password",
                salt: b"salt".to_vec(),
                rounds: 4,
                out: vec![
                    0x5b, 0xbf, 0x0c, 0xc2, 0x93, 0x58, 0x7f, 0x1c, 0x36, 0x35, 0x55, 0x5c, 0x27, 0x79, 0x65, 0x98,
                    0xd4, 0x7e, 0x57, 0x90, 0x71, 0xbf, 0x42, 0x7e, 0x9d, 0x8f, 0xbe, 0x84, 0x2a, 0xba, 0x34, 0xd9],
            }, Test{
                password: "password",
                salt: vec![0],
                rounds: 4,
                out: vec![0xc1, 0x2b, 0x56, 0x62, 0x35, 0xee, 0xe0, 0x4c, 0x21, 0x25, 0x98, 0x97, 0x0a, 0x57, 0x9a, 0x67],
            }, Test{
                password: "\x00",
                salt: b"salt".to_vec(),
                rounds: 4,
                out: vec![0x60, 0x51, 0xbe, 0x18, 0xc2, 0xf4, 0xf8, 0x2c, 0xbf, 0x0e, 0xfe, 0xe5, 0x47, 0x1b, 0x4b, 0xb9],
            }, Test{
                password: "password\x00",
                salt: b"salt\x00".to_vec(),
                rounds: 4,
                out: vec![
                    0x74, 0x10, 0xe4, 0x4c, 0xf4, 0xfa, 0x07, 0xbf, 0xaa, 0xc8, 0xa9, 0x28, 0xb1, 0x72, 0x7f, 0xac,
                    0x00, 0x13, 0x75, 0xe7, 0xbf, 0x73, 0x84, 0x37, 0x0f, 0x48, 0xef, 0xd1, 0x21, 0x74, 0x30, 0x50],
            }, Test{
                password: "pass\x00wor",
                salt: b"sa\x00l".to_vec(),
                rounds: 4,
                out: vec![0xc2, 0xbf, 0xfd, 0x9d, 0xb3, 0x8f, 0x65, 0x69, 0xef, 0xef, 0x43, 0x72, 0xf4, 0xde, 0x83, 0xc0],
            }, Test{
                password: "pass\x00word",
                salt: b"sa\x00lt".to_vec(),
                rounds: 4,
                out: vec![0x4b, 0xa4, 0xac, 0x39, 0x25, 0xc0, 0xe8, 0xd7, 0xf0, 0xcd, 0xb6, 0xbb, 0x16, 0x84, 0xa5, 0x6f],
            }, Test{
                password: "password",
                salt: b"salt".to_vec(),
                rounds: 8,
                out: vec![
                    0xe1, 0x36, 0x7e, 0xc5, 0x15, 0x1a, 0x33, 0xfa, 0xac, 0x4c, 0xc1, 0xc1, 0x44, 0xcd, 0x23, 0xfa,
                    0x15, 0xd5, 0x54, 0x84, 0x93, 0xec, 0xc9, 0x9b, 0x9b, 0x5d, 0x9c, 0x0d, 0x3b, 0x27, 0xbe, 0xc7,
                    0x62, 0x27, 0xea, 0x66, 0x08, 0x8b, 0x84, 0x9b, 0x20, 0xab, 0x7a, 0xa4, 0x78, 0x01, 0x02, 0x46,
                    0xe7, 0x4b, 0xba, 0x51, 0x72, 0x3f, 0xef, 0xa9, 0xf9, 0x47, 0x4d, 0x65, 0x08, 0x84, 0x5e, 0x8d],
            }, Test{
                password: "password",
                salt: b"salt".to_vec(),
                rounds: 42,
                out: vec![0x83, 0x3c, 0xf0, 0xdc, 0xf5, 0x6d, 0xb6, 0x56, 0x08, 0xe8, 0xf0, 0xdc, 0x0c, 0xe8, 0x82, 0xbd],
            }, Test{
                password: "Lorem ipsum dolor sit amet, consectetur adipisicing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.",
                salt: b"salis\x00".to_vec(),
                rounds: 8,
                out: vec![0x10, 0x97, 0x8b, 0x07, 0x25, 0x3d, 0xf5, 0x7f, 0x71, 0xa1, 0x62, 0xeb, 0x0e, 0x8a, 0xd3, 0x0a],
            },
        );

        for t in tests.iter() {
            let mut out = vec![0; t.out.len()];
            bcrypt_pbkdf(&t.password[..], &t.salt[..], t.rounds, &mut out);
            assert_eq!(out, t.out);
        }
    }
}
