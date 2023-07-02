use eva_common::prelude::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

// TODO nums are not fully supported for prod (no parsing/as string)
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TagId {
    Str(String),
    Num(u64),
}

impl TagId {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            TagId::Str(v) => Some(v.as_str()),
            TagId::Num(_) => None,
        }
    }
}

impl fmt::Display for TagId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TagId::Str(v) => write!(f, "{}", v),
            TagId::Num(v) => write!(f, "{}", v),
        }
    }
}

impl From<&str> for TagId {
    #[inline]
    fn from(s: &str) -> Self {
        Self::Str(s.to_owned())
    }
}

impl From<String> for TagId {
    #[inline]
    fn from(s: String) -> Self {
        Self::Str(s)
    }
}

impl From<u64> for TagId {
    #[inline]
    fn from(u: u64) -> Self {
        Self::Num(u)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct Range {
    from: Option<usize>,
    to: Option<usize>,
}

impl Range {
    pub fn new(from: Option<usize>, to: Option<usize>) -> Self {
        Self { from, to }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Tag {
    id: TagId,
    pub range: Range,
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)?;
        if self.has_range() {
            write!(f, "[")?;
            let mut written = false;
            if let Some(len) = self.range_len() {
                if len == 1 {
                    write!(f, "{}", self.range.from.unwrap_or_default())?;
                    written = true;
                }
            }
            if !written {
                if let Some(from) = self.range.from {
                    write!(f, "{}", from)?;
                }
                write!(f, "-")?;
                if let Some(to) = self.range.to {
                    write!(f, "{}", to)?;
                }
            }
            write!(f, "]")?;
        }
        Ok(())
    }
}

macro_rules! impl_to_tag {
    ($t: ty) => {
        impl From<$t> for Tag {
            #[inline]
            fn from(s: $t) -> Self {
                Self::new0(s.into())
            }
        }
    };
}

impl_to_tag!(&str);
impl_to_tag!(String);
impl_to_tag!(u64);

impl Tag {
    #[inline]
    pub fn new(id: TagId, range: Range) -> Self {
        Self { id, range }
    }
    #[inline]
    pub fn new0(id: TagId) -> Self {
        Self {
            id,
            range: Range::default(),
        }
    }
    #[inline]
    pub fn has_range(&self) -> bool {
        self.range.from.is_some() || self.range.to.is_some()
    }
    pub fn range_len(&self) -> Option<usize> {
        self.range
            .to
            .map(|to| to - self.range.from.unwrap_or_default() + 1)
    }
}

fn parse_range(s: &str) -> EResult<Range> {
    if let Some(pos) = s.find('-') {
        let f = &s[..pos];
        let t = &s[pos + 1..];
        let from = if f.is_empty() { None } else { Some(f.parse()?) };
        let to = if t.is_empty() { None } else { Some(t.parse()?) };
        if let Some(f) = from {
            if let Some(t) = to {
                if f > t {
                    return Err(Error::invalid_params("invalid seq index"));
                }
            }
        }
        Ok(Range::new(from, to))
    } else {
        let n: usize = s.parse()?;
        Ok(Range::new(Some(n), Some(n)))
    }
}

impl FromStr for Tag {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(x) = s.strip_suffix(']') {
            if let Some(pos) = x.rfind('[') {
                let range = parse_range(&x[pos + 1..])?;
                Ok(Tag::new(x[..pos].into(), range))
            } else {
                Err(Error::invalid_params("invalid array"))
            }
        } else {
            Ok(Tag::new0(s.into()))
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TagMap {
    tags: BTreeMap<TagId, Value>,
}

impl Serialize for TagMap {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.tags.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TagMap {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let tags: BTreeMap<TagId, Value> = Deserialize::deserialize(deserializer)?;
        Ok(Self { tags })
    }
}

impl TagMap {
    pub fn tags(&self) -> &BTreeMap<TagId, Value> {
        &self.tags
    }
    pub fn tag_mut(&mut self) -> &mut BTreeMap<TagId, Value> {
        &mut self.tags
    }
    pub fn delete(&mut self, tag: Tag) -> EResult<()> {
        if tag.has_range() {
            Err(Error::invalid_params("can not delete range"))
        } else {
            self.tags.remove(&tag.id);
            Ok(())
        }
    }
    pub fn get(&mut self, tag: &Tag) -> EResult<Value> {
        if let Some(val) = self.tags.get(&tag.id) {
            if tag.has_range() {
                if let Value::Seq(seq) = val {
                    if let Some(len) = tag.range_len() {
                        if len == 1 {
                            return Ok(seq
                                .get(tag.range.from.unwrap_or_default())
                                .map_or(Value::Unit, Clone::clone));
                        }
                    }
                    let from = tag.range.from.unwrap_or_default();
                    let to = tag.range.to.map_or_else(|| seq.len(), |v| v + 1);
                    let mut result = Vec::with_capacity(to - from + 1);
                    for i in from..to {
                        result.push(seq.get(i).map_or(Value::Unit, Clone::clone));
                    }
                    Ok(Value::Seq(result))
                } else {
                    Err(Error::invalid_data("tag is not a seq"))
                }
            } else {
                Ok(val.clone())
            }
        } else {
            Err(Error::not_found("no such tag"))
        }
    }
    pub fn set(&mut self, tag: Tag, value: Value) -> EResult<()> {
        if tag.has_range() {
            if let Some(val) = self.tags.get_mut(&tag.id) {
                // setting existing array tag
                if let Value::Seq(seq) = val {
                    if let Some(len) = tag.range_len() {
                        if len == 1 {
                            // replace a single el
                            let idx = tag.range.from.unwrap_or_default();
                            if seq.len() < idx + 1 {
                                seq.resize(idx + 1, Value::Unit);
                            }
                            seq[idx] = value;
                        } else if let Value::Seq(s) = value {
                            if s.len() != len {
                                return Err(Error::invalid_params("invalid value seq len"));
                            }
                            // set array part
                            let last_idx = tag.range.to.unwrap_or_default() + 1;
                            let tail = if last_idx > seq.len() {
                                None
                            } else {
                                Some(seq.split_off(last_idx))
                            };
                            let first_idx = tag.range.from.unwrap_or_default();
                            seq.resize(first_idx, Value::Unit);
                            seq.extend(s);
                            if let Some(t) = tail {
                                seq.extend(t);
                            }
                        } else {
                            return Err(Error::invalid_params("value is not a seq"));
                        }
                    } else if let Value::Seq(s) = value {
                        // no len given - we have starting index only
                        let idx = tag.range.from.unwrap_or_default();
                        seq.resize(idx, Value::Unit);
                        seq.extend(s);
                    } else {
                        return Err(Error::invalid_params("value is not a seq"));
                    }
                } else {
                    return Err(Error::invalid_params("tag is not an array"));
                }
            } else if let Value::Seq(seq) = value {
                let len = if let Some(len) = tag.range_len() {
                    if len != seq.len() {
                        return Err(Error::invalid_params("invalid value seq len"));
                    }
                    len
                } else {
                    tag.range.from.unwrap_or_default() + seq.len()
                };
                let mut result = Vec::with_capacity(len);
                result.resize(tag.range.from.unwrap_or_default(), Value::Unit);
                result.extend(seq);
                self.tags.insert(tag.id, Value::Seq(result));
            } else if let Some(len) = tag.range_len() {
                if len == 1 {
                    let idx = tag.range.from.unwrap_or_default();
                    let mut result = vec![Value::Unit; idx + 1];
                    result[idx] = value;
                    self.tags.insert(tag.id, Value::Seq(result));
                } else {
                    return Err(Error::invalid_params("value is not a seq"));
                }
            } else {
                return Err(Error::invalid_params("value is not a seq"));
            }
        } else {
            self.tags.insert(tag.id, value);
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::Tag;

    #[test]
    fn test_parse_display() {
        let tag: Tag = "test".parse().unwrap();
        assert_eq!(tag.to_string(), "test");

        let tag: Tag = "test[1]".parse().unwrap();
        assert_eq!(tag.to_string(), "test[1]");

        let tag: Tag = "test[-1]".parse().unwrap();
        assert_eq!(tag.to_string(), "test[-1]");

        let tag: Tag = "test[1-]".parse().unwrap();
        assert_eq!(tag.to_string(), "test[1-]");

        let tag: Tag = "test[1-5]".parse().unwrap();
        assert_eq!(tag.to_string(), "test[1-5]");
    }
}
