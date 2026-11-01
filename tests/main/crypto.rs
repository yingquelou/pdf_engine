use super::*;

fn rc4_5(key: &[u8], input: &[u8]) -> Vec<u8> {
    let mut cipher = Rc4::<U5>::new_from_slice(key).unwrap();
    let mut output = input.to_vec();
    cipher.apply_keystream(&mut output);
    output
}

fn rc4_10(key: &[u8], input: &[u8]) -> Vec<u8> {
    let mut cipher = Rc4::<U10>::new_from_slice(key).unwrap();
    let mut output = input.to_vec();
    cipher.apply_keystream(&mut output);
    output
}

fn rc4_16(key: &[u8], input: &[u8]) -> Vec<u8> {
    let mut cipher = Rc4::<U16>::new_from_slice(key).unwrap();
    let mut output = input.to_vec();
    cipher.apply_keystream(&mut output);
    output
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}

fn encrypt_r2_fixture() -> Vec<u8> {
    const PADDING: [u8; 32] = [
        0x28, 0xbf, 0x4e, 0x5e, 0x4e, 0x75, 0x8a, 0x41, 0x64, 0x00, 0x4e, 0x56, 0xff, 0xfa, 0x01,
        0x08, 0x2e, 0x2e, 0x00, 0xb6, 0xd0, 0x68, 0x3e, 0x80, 0x2f, 0x0c, 0xa9, 0xfe, 0x64, 0x53,
        0x69, 0x7a,
    ];
    let pad = |password: &[u8]| {
        let mut result = password[..password.len().min(32)].to_vec();
        result.extend_from_slice(&PADDING[..32 - result.len()]);
        result
    };
    let id = b"pdf-security-test";
    let user_pad = pad(b"user-password");
    let owner_pad = pad(b"owner-password");
    let owner_key = Md5::digest(&owner_pad);
    let owner_entry = rc4_5(&owner_key[..5], &user_pad);
    let permissions = -4i32;
    let mut key_material = user_pad;
    key_material.extend_from_slice(&owner_entry);
    key_material.extend_from_slice(&permissions.to_le_bytes());
    key_material.extend_from_slice(id);
    let file_key = Md5::digest(key_material);
    let user_entry = rc4_5(&file_key[..5], &PADDING);

    let mut object_key_material = file_key[..5].to_vec();
    object_key_material.extend_from_slice(&[2, 0, 0, 0, 0]);
    let object_key = Md5::digest(object_key_material);
    let encrypted_label = rc4_10(&object_key[..10], b"decrypted label");
    let encrypted_stream = rc4_10(&object_key[..10], b"decrypted stream");

    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(
        format!(
            "2 0 obj\n<< /Label <{}> /Length {} >>\nstream\n",
            hex_bytes(&encrypted_label),
            encrypted_stream.len()
        )
        .as_bytes(),
    );
    buf.extend_from_slice(&encrypted_stream);
    buf.extend_from_slice(b"\nendstream\nendobj\n");
    let off3 = buf.len();
    buf.extend_from_slice(
        format!(
            "3 0 obj\n<< /Filter /Standard /V 1 /R 2 /O <{}> /U <{}> /P {} >>\nendobj\n",
            hex_bytes(&owner_entry),
            hex_bytes(&user_entry),
            permissions
        )
        .as_bytes(),
    );
    let off_xref = buf.len();
    buf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{off1:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{off2:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{off3:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(
        format!(
            "trailer\n<< /Size 4 /Root 1 0 R /Encrypt 3 0 R /ID [<{}> <{}>] >>\nstartxref\n{off_xref}\n%%EOF\n",
            hex_bytes(id),
            hex_bytes(id)
        )
        .as_bytes(),
    );
    buf
}

/// ISO 32000-1 Standard Security Handler R2 authenticates user and owner
/// passwords, decrypts object strings and streams, and refuses unsafe writes.
#[test]
fn standard_security_r2_decrypts_strings_and_streams() {
    let buf = encrypt_r2_fixture();
    let doc = parse_buf_with_password(&buf, b"user-password");
    assert!(doc.is_encrypted());
    let s = doc.get_object(2).expect("parse").expect("stream present");
    let label = s.value.as_dict().unwrap().get("Label").unwrap();
    assert_eq!(label, &Value::HexString(hex_bytes(b"decrypted label")));
    assert_eq!(doc.try_stream_bytes(&s.value).unwrap(), b"decrypted stream");
    assert_eq!(
        doc.decode_stream_bytes(&s.value).unwrap(),
        b"decrypted stream"
    );

    let owner_doc = parse_buf_with_password(&buf, b"owner-password");
    let owner_stream = owner_doc.get_object(2).unwrap().unwrap();
    assert_eq!(
        owner_doc.try_stream_bytes(&owner_stream.value).unwrap(),
        b"decrypted stream"
    );
    assert!(parse_buf_result(&buf, Some(b"wrong-password")).is_err());

    let error = pdf_engine::core::writer::DocumentWriter::new()
        .build(&doc, &mut Vec::new())
        .unwrap_err();
    assert!(matches!(
        error,
        pdf_engine::core::writer::WriteError::EncryptedDocument
    ));

    // 加密特性被记录
    let feats = doc.features.borrow();
    assert!(
        feats.contains(PdfFeature::EncryptionRC4),
        "trailer /Encrypt 应记录 EncryptionRC4"
    );
    drop(feats);
}

#[test]
fn standard_security_r4_decrypts_explicit_crypt_filter() {
    let buf = encrypt_r4_fixture();
    let doc = parse_buf_with_password(&buf, b"user-password");
    let stream = doc.get_object(2).unwrap().unwrap();
    let label = stream.value.as_dict().unwrap().get("Label").unwrap();
    assert_eq!(label, &Value::HexString(hex_bytes(b"R4 encrypted label")));
    assert_eq!(
        doc.try_stream_bytes(&stream.value).unwrap(),
        b"R4 crypt filter stream"
    );
    assert_eq!(
        doc.decode_stream_bytes(&stream.value).unwrap(),
        b"R4 crypt filter stream"
    );
    assert!(parse_buf_result(&buf, Some(b"wrong-password")).is_err());
}

#[test]
fn crypt_identity_filter_is_supported_without_encryption() {
    let doc = Document::new();
    let stream = Value::Stream(
        Dict::new()
            .insert("Filter", Value::Name("Crypt".into()))
            .insert(
                "DP",
                Value::Dict(Dict::new().insert("Name", Value::Name("Identity".into()))),
            ),
        StreamData::Owned(b"identity stream".to_vec()),
    );
    assert_eq!(
        doc.decode_stream_bytes(&stream).unwrap(),
        b"identity stream"
    );

    let non_identity = Value::Stream(
        Dict::new()
            .insert("Filter", Value::Name("Crypt".into()))
            .insert(
                "DecodeParms",
                Value::Dict(Dict::new().insert("Name", Value::Name("StdCF".into()))),
            ),
        StreamData::Owned(b"ciphertext".to_vec()),
    );
    assert!(doc.decode_stream_bytes(&non_identity).is_err());

    let invalid_params = Value::Stream(
        Dict::new()
            .insert(
                "Filter",
                Value::Array(Array(vec![
                    Value::Name("Crypt".into()),
                    Value::Name("FlateDecode".into()),
                ])),
            )
            .insert(
                "DecodeParms",
                Value::Dict(Dict::new().insert("Name", Value::Name("Identity".into()))),
            ),
        StreamData::Owned(Vec::new()),
    );
    assert!(doc.decode_stream_bytes(&invalid_params).is_err());
}

fn encrypt_r4_fixture() -> Vec<u8> {
    const PADDING: [u8; 32] = [
        0x28, 0xbf, 0x4e, 0x5e, 0x4e, 0x75, 0x8a, 0x41, 0x64, 0x00, 0x4e, 0x56, 0xff, 0xfa, 0x01,
        0x08, 0x2e, 0x2e, 0x00, 0xb6, 0xd0, 0x68, 0x3e, 0x80, 0x2f, 0x0c, 0xa9, 0xfe, 0x64, 0x53,
        0x69, 0x7a,
    ];
    let pad = |password: &[u8]| {
        let mut result = password[..password.len().min(32)].to_vec();
        result.extend_from_slice(&PADDING[..32 - result.len()]);
        result
    };
    let id = b"pdf-r4-security-test";
    let user_password = pad(b"user-password");
    let owner_password = pad(b"owner-password");
    let mut owner_digest = Md5::digest(&owner_password).to_vec();
    for _ in 0..50 {
        owner_digest = Md5::digest(&owner_digest[..16]).to_vec();
    }
    let mut owner_entry = rc4_16(&owner_digest[..16], &user_password);
    for iteration in 1..20u8 {
        let key: Vec<u8> = owner_digest[..16]
            .iter()
            .map(|byte| byte ^ iteration)
            .collect();
        owner_entry = rc4_16(&key, &owner_entry);
    }

    let permissions = -4i32;
    let mut key_material = user_password;
    key_material.extend_from_slice(&owner_entry);
    key_material.extend_from_slice(&permissions.to_le_bytes());
    key_material.extend_from_slice(id);
    key_material.extend_from_slice(&[0xff; 4]);
    let mut file_key = Md5::digest(key_material).to_vec();
    for _ in 0..50 {
        file_key = Md5::digest(&file_key[..16]).to_vec();
    }
    let mut user_material = PADDING.to_vec();
    user_material.extend_from_slice(id);
    let digest = Md5::digest(user_material);
    let mut user_entry = rc4_16(&file_key[..16], &digest);
    for iteration in 1..20u8 {
        let key: Vec<u8> = file_key[..16].iter().map(|byte| byte ^ iteration).collect();
        user_entry = rc4_16(&key, &user_entry);
    }
    user_entry.extend_from_slice(&[0; 16]);

    let mut object_key_material = file_key[..16].to_vec();
    object_key_material.extend_from_slice(&[2, 0, 0, 0, 0]);
    object_key_material.extend_from_slice(b"sAlT");
    let object_key = Md5::digest(object_key_material);
    let iv = [0x73; 16];
    let plaintext = b"R4 crypt filter stream";
    let encrypted_label = encrypt_aesv2(b"R4 encrypted label", &object_key[..16], [0x29; 16]);
    let encrypted_stream = encrypt_aesv2(plaintext, &object_key[..16], iv);
    let mut buf = b"%PDF-1.7\n".to_vec();
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(
        format!(
            "2 0 obj\n<< /Label <{}> /Filter /Crypt /DecodeParms << /Name /StdCF >> /Length {} >>\nstream\n",
            hex_bytes(&encrypted_label),
            encrypted_stream.len()
        )
        .as_bytes(),
    );
    buf.extend_from_slice(&encrypted_stream);
    buf.extend_from_slice(b"\nendstream\nendobj\n");
    let off3 = buf.len();
    buf.extend_from_slice(
        format!(
            "3 0 obj\n<< /Filter /Standard /V 4 /R 4 /Length 128 /O <{}> /U <{}> /P {} /EncryptMetadata false /CF << /StdCF << /CFM /AESV2 /Length 16 >> >> /StmF /StdCF /StrF /StdCF >>\nendobj\n",
            hex_bytes(&owner_entry),
            hex_bytes(&user_entry),
            permissions
        )
        .as_bytes(),
    );
    let off_xref = buf.len();
    buf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{off1:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{off2:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{off3:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(
        format!(
            "trailer\n<< /Size 4 /Root 1 0 R /Encrypt 3 0 R /ID [<{}> <{}>] >>\nstartxref\n{off_xref}\n%%EOF\n",
            hex_bytes(id),
            hex_bytes(id)
        )
        .as_bytes(),
    );
    buf
}

fn encrypt_aesv2(plaintext: &[u8], key: &[u8], iv: [u8; 16]) -> Vec<u8> {
    let padding = 16 - plaintext.len() % 16;
    let mut ciphertext = plaintext.to_vec();
    ciphertext.extend(std::iter::repeat_n(padding as u8, padding));
    let cipher = Aes128::new_from_slice(key).unwrap();
    let mut previous = iv;
    for chunk in ciphertext.chunks_mut(16) {
        for (byte, prior) in chunk.iter_mut().zip(previous) {
            *byte ^= prior;
        }
        let mut block = GenericArray::clone_from_slice(chunk);
        cipher.encrypt_block(&mut block);
        chunk.copy_from_slice(&block);
        previous.copy_from_slice(chunk);
    }
    [iv.as_slice(), ciphertext.as_slice()].concat()
}
