// versions.rs -- core Portage functionality
// Converted from Python to Rust

use regex::Regex;
use crate::exception::InvalidData;

// Lazy static for regexes
lazy_static::lazy_static! {
    static ref VER_REGEXP: Regex = Regex::new(r"^(\d+)((\.\d+)*)([a-z]?)((_(pre|p|beta|alpha|rc)\d*)*)(-r(\d+))?$").unwrap();
    static ref SUFFIX_REGEXP: Regex = Regex::new(r"^(alpha|beta|rc|pre|p)(\d*)$").unwrap();
    static ref PV_RE: Regex = Regex::new(r"^(?P<pn>[\w+][\w+-]*?(?P<pn_inval>-\d+(\.\d+)*([a-z]?)((_(pre|p|beta|alpha|rc)\d*)*))?)-(?P<ver>\d+(\.\d+)*([a-z]?)((_(pre|p|beta|alpha|rc)\d*)*))(-r(?P<rev>\d+))?$").unwrap();
    static ref CAT_RE: Regex = Regex::new(r"^[\w+][\w+.-]*$").unwrap();
}

static SUFFIX_VALUE: phf::Map<&'static str, i32> = phf::phf_map! {
    "pre" => -2,
    "p" => 0,
    "alpha" => -4,
    "beta" => -3,
    "rc" => -1,
};

const MISSING_CAT: &str = "null";

pub fn ververify(myver: &str) -> bool {
    VER_REGEXP.is_match(myver)
}

pub fn vercmp(ver1: &str, ver2: &str) -> Option<i32> {
    if ver1 == ver2 {
        return Some(0);
    }

    let match1 = VER_REGEXP.captures(ver1)?;
    let match2 = VER_REGEXP.captures(ver2)?;

    // Build lists of version parts
    let mut list1: Vec<i64> = vec![match1.get(1)?.as_str().parse().ok()?];
    let mut list2: Vec<i64> = vec![match2.get(1)?.as_str().parse().ok()?];

    // Handle dotted parts
    if let (Some(g2_1), Some(g2_2)) = (match1.get(2), match2.get(2)) {
        let g2_1_str = g2_1.as_str();
        let g2_2_str = g2_2.as_str();

        let vlist1: Vec<&str> = if g2_1_str.is_empty() {
            vec![]
        } else {
            g2_1_str[1..].split('.').filter(|s| !s.is_empty()).collect()
        };

        let vlist2: Vec<&str> = if g2_2_str.is_empty() {
            vec![]
        } else {
            g2_2_str[1..].split('.').filter(|s| !s.is_empty()).collect()
        };

        for i in 0..std::cmp::max(vlist1.len(), vlist2.len()) {
            if i >= vlist1.len() || vlist1[i].is_empty() {
                list1.push(-1);
                if i < vlist2.len() {
                    list2.push(vlist2[i].parse().unwrap_or(0));
                } else {
                    list2.push(0);
                }
            } else if i >= vlist2.len() || vlist2[i].is_empty() {
                list2.push(-1);
                list1.push(vlist1[i].parse().unwrap_or(0));
            } else {
                let s1 = vlist1[i];
                let s2 = vlist2[i];
                if !s1.starts_with('0') && !s2.starts_with('0') {
                    list1.push(s1.parse().unwrap_or(0));
                    list2.push(s2.parse().unwrap_or(0));
                } else {
                    let max_len = std::cmp::max(s1.len(), s2.len());
                    list1.push(format!("{:0>width$}", s1, width = max_len).parse().unwrap_or(0));
                    list2.push(format!("{:0>width$}", s2, width = max_len).parse().unwrap_or(0));
                }
            }
        }
    }

    // Final letter
    if let Some(g4) = match1.get(4) {
        if !g4.as_str().is_empty() {
            list1.push(g4.as_str().chars().next().unwrap() as i64);
        }
    }
    if let Some(g4) = match2.get(4) {
        if !g4.as_str().is_empty() {
            list2.push(g4.as_str().chars().next().unwrap() as i64);
        }
    }

    for i in 0..std::cmp::max(list1.len(), list2.len()) {
        let a = list1.get(i).copied().unwrap_or(0);
        let b = list2.get(i).copied().unwrap_or(0);
        if a != b {
            return Some(if a > b { 1 } else { -1 });
        }
    }

    // Suffix part
    let list1_suffix: Vec<&str> = match1.get(5).map(|m| m.as_str().split('_').skip(1).collect()).unwrap_or_default();
    let list2_suffix: Vec<&str> = match2.get(5).map(|m| m.as_str().split('_').skip(1).collect()).unwrap_or_default();

    for i in 0..std::cmp::max(list1_suffix.len(), list2_suffix.len()) {
        let s1 = if i >= list1_suffix.len() {
            ("p", "-1")
        } else {
            SUFFIX_REGEXP.captures(list1_suffix[i]).map(|c| (c.get(1).unwrap().as_str(), c.get(2).map(|m| m.as_str()).unwrap_or("-1"))).unwrap_or(("p", "-1"))
        };
        let s2 = if i >= list2_suffix.len() {
            ("p", "-1")
        } else {
            SUFFIX_REGEXP.captures(list2_suffix[i]).map(|c| (c.get(1).unwrap().as_str(), c.get(2).map(|m| m.as_str()).unwrap_or("-1"))).unwrap_or(("p", "-1"))
        };

        if s1.0 != s2.0 {
            let a = SUFFIX_VALUE.get(s1.0).copied().unwrap_or(0);
            let b = SUFFIX_VALUE.get(s2.0).copied().unwrap_or(0);
            return Some(if a > b { 1 } else { -1 });
        }

        let r1: i32 = s1.1.parse().unwrap_or(0);
        let r2: i32 = s2.1.parse().unwrap_or(0);
        if r1 != r2 {
            return Some(if r1 > r2 { 1 } else { -1 });
        }
    }

    // Revision
    let r1: i32 = match1.get(9).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
    let r2: i32 = match2.get(9).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
    Some(if r1 > r2 { 1 } else if r1 < r2 { -1 } else { 0 })
}



pub fn best(mymatches: &[String]) -> String {
    if mymatches.is_empty() {
        return "".to_string();
    }
    mymatches.iter().max_by(|a, b| vercmp(a, b).unwrap_or(0).cmp(&0)).unwrap().clone()
}

fn _pkgsplit(mypkg: &str) -> Option<(String, String, String)> {
    let m = PV_RE.captures(mypkg)?;
    if m.name("pn_inval").is_some() {
        return None;
    }
    let pn = m.name("pn")?.as_str().to_string();
    let ver = m.name("ver")?.as_str().to_string();
    let rev = if let Some(r) = m.name("rev") {
        format!("r{}", r.as_str())
    } else {
        "r0".to_string()
    };
    Some((pn, ver, rev))
}

pub fn catpkgsplit(mydata: &str) -> Option<Vec<String>> {
    let mysplit: Vec<&str> = mydata.split('/').collect();
    let (cat, p_str) = if mysplit.len() == 1 {
        (MISSING_CAT.to_string(), mysplit[0])
    } else if mysplit.len() == 2 {
        if CAT_RE.is_match(mysplit[0]) {
            (mysplit[0].to_string(), mysplit[1])
        } else {
            return None;
        }
    } else {
        return None;
    };
    let p_split = _pkgsplit(p_str)?;
    Some(vec![cat, p_split.0, p_split.1, p_split.2])
}

pub fn pkgsplit(mypkg: &str) -> Option<(String, String, String)> {
    let catpsplit = catpkgsplit(mypkg)?;
    let cat = &catpsplit[0];
    let pn = &catpsplit[1];
    let ver = &catpsplit[2];
    let rev = &catpsplit[3];
    if cat == MISSING_CAT && !mypkg.contains('/') {
        Some((pn.clone(), ver.clone(), rev.clone()))
    } else {
        Some((format!("{}/{}", cat, pn), ver.clone(), rev.clone()))
    }
}

pub fn cpv_getkey(mycpv: &str) -> Option<String> {
    let mysplit = catpkgsplit(mycpv)?;
    Some(format!("{}/{}", mysplit[0], mysplit[1]))
}

pub fn cpv_getversion(mycpv: &str) -> Option<String> {
    let cp = cpv_getkey(mycpv)?;
    Some(mycpv[cp.len() + 1..].to_string())
}

pub fn catsplit(mydep: &str) -> Vec<String> {
    mydep.split('/').map(|s| s.to_string()).collect()
}

pub fn pkgcmp(pkg1: (&str, &str, &str), pkg2: (&str, &str, &str)) -> Option<i32> {
    if pkg1.0 != pkg2.0 {
        return None;
    }
    vercmp(&format!("{}-{}", pkg1.1, pkg1.2), &format!("{}-{}", pkg2.1, pkg2.2))
}

#[derive(Debug, Clone)]
pub struct PkgStr {
    pub cpv: String,
    pub cpv_split: Vec<String>,
    pub cp: String,
    pub version: String,
    pub slot: Option<String>,
    pub sub_slot: Option<String>,
    pub repo: Option<String>,
    // Add other fields as needed
}

impl PkgStr {
    pub fn new(cpv: &str) -> Result<Self, InvalidData> {
        let cpv_split = catpkgsplit(cpv).ok_or_else(|| InvalidData::new(cpv, None))?;
        let cp = format!("{}/{}", cpv_split[0], cpv_split[1]);
        let version = if cpv_split[3] == "r0" && !cpv.ends_with("-r0") {
            cpv_split[2].clone()
        } else {
            format!("{}-{}", cpv_split[2], cpv_split[3])
        };
        Ok(PkgStr {
            cpv: cpv.to_string(),
            cpv_split,
            cp,
            version,
            slot: None,
            sub_slot: None,
            repo: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_vercmp() {
        // Equal versions
        assert_eq!(vercmp("1.0.0", "1.0.0"), Some(0));

        // Simple comparisons
        assert_eq!(vercmp("1.0.0", "1.0.1"), Some(-1));
        assert_eq!(vercmp("1.0.1", "1.0.0"), Some(1));

        // Different number of dots
        assert_eq!(vercmp("1.0", "1.0.1"), Some(-1));
    }

    #[tokio::test]
    async fn test_ververify() {
        assert!(ververify("1.0.0"));
        assert!(ververify("1.0.0-r1"));
        assert!(ververify("1.0.0_alpha"));
        assert!(!ververify(""));
        assert!(!ververify("invalid"));
    }
}