//! Compact QR transport only. The TLS identity, challenge and signed invitation
//! are unchanged; no certificate verification or pairing checks are removed.
use super::*;
pub(super) const PREFIX: &str = "BOBPAIR2:";
const ALPHABET: &[u8; 45] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";
fn base45(bytes: &[u8]) -> String {
    let mut out = String::new();
    for pair in bytes.chunks(2) {
        let mut n = if pair.len() == 2 {
            256 * pair[0] as usize + pair[1] as usize
        } else {
            pair[0] as usize
        };
        out.push(ALPHABET[n % 45] as char);
        n /= 45;
        out.push(ALPHABET[n % 45] as char);
        n /= 45;
        if pair.len() == 2 {
            out.push(ALPHABET[n] as char);
        }
    }
    out
}
fn unbase45(text: &str) -> Result<Vec<u8>, &'static str> {
    if text.len() % 3 == 1 {
        return Err("PAIRING_CODE_INVALID");
    }
    let mut out = Vec::new();
    for group in text.as_bytes().chunks(3) {
        let mut n = 0usize;
        let mut weight = 1;
        for b in group {
            let v = ALPHABET
                .iter()
                .position(|x| x == b)
                .ok_or("PAIRING_CODE_INVALID")?;
            n += v * weight;
            weight *= 45;
        }
        if group.len() == 3 {
            if n > 65535 {
                return Err("PAIRING_CODE_INVALID");
            }
            out.push((n / 256) as u8);
            out.push((n % 256) as u8);
        } else {
            if n > 255 {
                return Err("PAIRING_CODE_INVALID");
            }
            out.push(n as u8);
        }
    }
    Ok(out)
}
fn field(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), &'static str> {
    let n = u16::try_from(bytes.len()).map_err(|_| "PAIRING_CODE_INVALID")?;
    out.extend_from_slice(&n.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}
struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], &'static str> {
        let end = self.offset.checked_add(n).ok_or("PAIRING_CODE_INVALID")?;
        let v = self
            .bytes
            .get(self.offset..end)
            .ok_or("PAIRING_CODE_INVALID")?;
        self.offset = end;
        Ok(v)
    }
    fn u16(&mut self) -> Result<u16, &'static str> {
        Ok(u16::from_be_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| "PAIRING_CODE_INVALID")?,
        ))
    }
    fn i64(&mut self) -> Result<i64, &'static str> {
        Ok(i64::from_be_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| "PAIRING_CODE_INVALID")?,
        ))
    }
    fn field(&mut self) -> Result<&'a [u8], &'static str> {
        let n = self.u16()? as usize;
        self.take(n)
    }
    fn text(&mut self) -> Result<String, &'static str> {
        String::from_utf8(self.field()?.to_vec()).map_err(|_| "PAIRING_CODE_INVALID")
    }
}
pub(super) fn encode(code: &QrCode) -> Result<String, &'static str> {
    let challenge = code.challenge()?;
    if code.i != format!("invite-{:x}", Sha256::digest(challenge)) {
        return Err("PAIRING_CODE_INVALID");
    }
    let mut b = vec![2];
    field(&mut b, code.h.as_bytes())?;
    b.extend_from_slice(&code.e.to_be_bytes());
    let identity = code.home()?;
    field(&mut b, &identity.certificate_der)?;
    field(&mut b, code.n.as_bytes())?;
    b.extend_from_slice(&challenge);
    b.extend_from_slice(&code.x.to_be_bytes());
    b.push(u8::try_from(code.a.len()).map_err(|_| "PAIRING_CODE_INVALID")?);
    for a in &code.a {
        field(&mut b, a.to_string().as_bytes())?;
    }
    b.extend_from_slice(&code.f.to_be_bytes());
    b.extend_from_slice(&code.w.to_be_bytes());
    Ok(format!("{PREFIX}{}", base45(&b)))
}
pub(super) fn decode(text: &str, at: i64) -> Result<QrCode, &'static str> {
    let b = unbase45(text.strip_prefix(PREFIX).ok_or("PAIRING_CODE_INVALID")?)?;
    let mut r = Reader {
        bytes: &b,
        offset: 0,
    };
    if r.take(1)? != [2] {
        return Err("PAIRING_CODE_INVALID");
    }
    let h = r.text()?;
    let e = r.i64()?;
    let c = URL_SAFE_NO_PAD.encode(r.field()?);
    let n = r.text()?;
    let challenge = r.take(32)?.to_vec();
    let x = r.i64()?;
    let count = r.take(1)?[0];
    if count == 0 || count > 4 {
        return Err("PAIRING_CODE_INVALID");
    }
    let mut a = Vec::new();
    for _ in 0..count {
        a.push(r.text()?.parse().map_err(|_| "PAIRING_CODE_INVALID")?);
    }
    let f = r.u16()?;
    let w = r.u16()?;
    if r.offset != b.len() {
        return Err("PAIRING_CODE_INVALID");
    }
    let value = QrCode {
        v: 1,
        i: format!("invite-{:x}", Sha256::digest(&challenge)),
        h,
        e,
        c,
        n,
        k: URL_SAFE_NO_PAD.encode(&challenge),
        x,
        a,
        f,
        w,
    };
    // Reuse all existing expiry, bounds, identity and allowed-address validation.
    let legacy = format!(
        "{}{}",
        super::PREFIX,
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&value).map_err(|_| "PAIRING_CODE_INVALID")?)
    );
    QrCode::decode(&legacy, at)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_binary_values_roundtrip_and_malformed_is_rejected() {
        for n in 0..600 {
            let b: Vec<u8> = (0..n).map(|i| (i % 256) as u8).collect();
            assert_eq!(unbase45(&base45(&b)).unwrap(), b);
        }
        for bad in ["A", "::: pair", "%%%", "::", "aaa"] {
            assert!(unbase45(bad).is_err());
        }
        assert!(decode("BOBPAIR2:00", 0).is_err());
    }
}
