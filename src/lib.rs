use eva_common::prelude::*;
use std::collections::BTreeMap;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub enum TagId {
    Str(String),
    Num(u64),
    Guid(Uuid),
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

impl From<Uuid> for TagId {
    #[inline]
    fn from(u: Uuid) -> Self {
        Self::Guid(u)
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
impl_to_tag!(Uuid);

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

impl TagMap {
    pub fn get(&mut self, tag: Tag) -> EResult<Value> {
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
                                seq[idx] = value;
                            }
                        } else if let Value::Seq(s) = value {
                            if s.len() != len {
                                return Err(Error::invalid_params("invalid value seq len"));
                            }
                            // set array part
                            let last_idx = tag.range.to.unwrap_or_default();
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
                    let mut result = Vec::with_capacity(tag.range.from.unwrap_or_default());
                    result.resize(tag.range.from.unwrap_or_default(), Value::Unit);
                    result.push(value);
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
