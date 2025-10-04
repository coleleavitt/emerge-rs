// xpak.rs -- XPAK archive handling

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

pub fn encodeint(myint: u32) -> [u8; 4] {
    [
        ((myint >> 24) & 0xFF) as u8,
        ((myint >> 16) & 0xFF) as u8,
        ((myint >> 8) & 0xFF) as u8,
        (myint & 0xFF) as u8,
    ]
}

pub fn decodeint(mystring: &[u8]) -> u32 {
    (mystring[3] as u32) +
    ((mystring[2] as u32) << 8) +
    ((mystring[1] as u32) << 16) +
    ((mystring[0] as u32) << 24)
}

pub fn xpak_mem(mydata: &HashMap<String, Vec<u8>>) -> Vec<u8> {
    let mut indexglob = Vec::new();
    let mut dataglob = Vec::new();
    let mut datapos = 0;

    for (x, newglob) in mydata {
        let mydatasize = newglob.len() as u32;
        indexglob.extend_from_slice(&encodeint(x.len() as u32));
        indexglob.extend_from_slice(x.as_bytes());
        indexglob.extend_from_slice(&encodeint(datapos));
        indexglob.extend_from_slice(&encodeint(mydatasize));
        dataglob.extend_from_slice(newglob);
        datapos += mydatasize;
    }

    let mut result = b"XPAKPACK".to_vec();
    result.extend_from_slice(&encodeint(indexglob.len() as u32));
    result.extend_from_slice(&encodeint(dataglob.len() as u32));
    result.extend_from_slice(&indexglob);
    result.extend_from_slice(&dataglob);
    result.extend_from_slice(b"XPAKSTOP");
    result
}

pub fn xsplit_mem(mydat: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    if mydat.len() < 16 || &mydat[0..8] != b"XPAKPACK" || &mydat[mydat.len()-8..] != b"XPAKSTOP" {
        return None;
    }
    let indexsize = decodeint(&mydat[8..12]) as usize;
    let index = mydat[16..16+indexsize].to_vec();
    let data = mydat[16+indexsize..mydat.len()-8].to_vec();
    Some((index, data))
}

pub fn getindex_mem(myindex: &[u8]) -> Vec<String> {
    let mut myret = Vec::new();
    let mut startpos = 0;
    while startpos + 8 < myindex.len() {
        let mytestlen = decodeint(&myindex[startpos..startpos+4]) as usize;
        let name = String::from_utf8_lossy(&myindex[startpos+4..startpos+4+mytestlen]).to_string();
        myret.push(name);
        startpos += mytestlen + 12;
    }
    myret
}

pub fn searchindex(myindex: &[u8], myitem: &str) -> Option<(usize, usize)> {
    let myitem_bytes = myitem.as_bytes();
    let mylen = myitem_bytes.len();
    let mut startpos = 0;
    while startpos + 8 < myindex.len() {
        let mytestlen = decodeint(&myindex[startpos..startpos+4]) as usize;
        if mytestlen == mylen && &myindex[startpos+4..startpos+4+mytestlen] == myitem_bytes {
            let datapos = decodeint(&myindex[startpos+4+mytestlen..startpos+8+mytestlen]) as usize;
            let datalen = decodeint(&myindex[startpos+8+mytestlen..startpos+12+mytestlen]) as usize;
            return Some((datapos, datalen));
        }
        startpos += mytestlen + 12;
    }
    None
}

pub fn getitem(myid: (&[u8], &[u8]), myitem: &str) -> Option<Vec<u8>> {
    let (myindex, mydata) = myid;
    if let Some((datapos, datalen)) = searchindex(myindex, myitem) {
        Some(mydata[datapos..datapos+datalen].to_vec())
    } else {
        None
    }
}

/// Create XPAK data from a directory (for binary package creation)
pub fn xpak(rootdir: &Path, outfile: Option<&Path>) -> Option<Vec<u8>> {
    // For binary packages, we don't need to xpak the entire directory
    // The XPAK data is metadata only, created separately
    None
}