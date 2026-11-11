// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded, disposable OIB inspector inputs. No method writes replicated state.
//!
//! Typical routing: `open(info, revision)`; `focus(field_id)`; `edit(field_id,
//! &edit)`; on Enter/valid blur/Apply call `prepare(address, fresh_info,
//! revision)`. Validate the candidate's full address and authority write against
//! the current snapshot, submit once, then `acknowledge(&candidate, accepted)`.
//! Escape calls `cancel(address)`. Selection changes never change draft identity.
//! The revision is a caller-defined snapshot/source marker, not authority proof.
use crate::canvas::oib::document::PropertyInfo;
use crate::dashboard::{font::Font, keyboard::TextBuffer};
use otrios_interfaces::keyboard::{Edit, TextInput};
use otrios_interfaces::layout::{Value, ValueType};
use otrios_interfaces::oib::designer::{PropertyAddress, PropertyDescriptor, PropertyEditor};

pub(crate) const MAX_DRAFTS: usize = 16;
pub(crate) const RAW_BYTES: usize = 256;
pub(crate) const TEXT_SCALE: f32 = 2.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    Value,
    Hex,
    /// Alpha as a percentage, inclusive 0..100; rounded once to RGBA8.
    Opacity,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Focus {
    pub address: PropertyAddress,
    pub part: Part,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    Capacity,
    IdExhausted,
    UnknownField,
    Unavailable,
    ReadOnly,
    InvalidText,
    InvalidNumber,
    InvalidColor,
    InvalidOpacity,
    RawLimit,
    StaleCandidate,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditResult {
    Local(bool),
    CommitRequested,
    Traverse { backward: bool },
}

#[derive(Clone)]
pub(crate) struct Field {
    pub id: u32,
    pub part: Part,
    buffer: TextBuffer,
}
impl Field {
    pub fn buffer(&self) -> &TextBuffer {
        &self.buffer
    }
    /// Uses actual clipped control bounds supplied by the caller. Insets and
    /// metrics match scale-2 local painting; callers retain responsibility for clip.
    pub fn input(&self, focused: bool, bounds: [f32; 4], font: &Font) -> TextInput {
        let mut input = self.buffer.host_input(self.id, focused);
        input.bounds = bounds;
        input.inset = [8., 8., 8., 8.];
        input.font_size = font.metrics().cache_em(TEXT_SCALE);
        input.line_height = font.metrics().line_height(TEXT_SCALE);
        input
    }
}

#[derive(Clone)]
pub(crate) struct Draft {
    pub address: PropertyAddress,
    pub fields: Vec<Field>,
    pub dirty: bool,
    pub error: Option<Error>,
    pub base_revision: u64,
    pub source_revision: u64,
    descriptor: PropertyDescriptor,
    boolean: bool,
    available: bool,
    pending: Option<Candidate>,
}
impl Draft {
    pub fn field(&self, part: Part) -> Option<&Field> {
        self.fields.iter().find(|f| f.part == part)
    }
    /// A disposable field/picker preview. Invalid raw input returns an error;
    /// the parent should continue showing the confirmed canvas in that case.
    pub fn preview(&self) -> Result<Value, Error> {
        let raw = |part| self.field(part).map(|f| f.buffer.text()).unwrap_or("");
        match self.descriptor.value_type {
            ValueType::Text => {
                let text = raw(Part::Value);
                if text.is_empty() || text.len() > 96 || text.contains('\0') {
                    Err(Error::InvalidText)
                } else {
                    Ok(Value::Text(text.into()))
                }
            }
            ValueType::Number => {
                let n = parse_number(raw(Part::Value)).ok_or(Error::InvalidNumber)?;
                let maximum =
                    if matches!(
                        self.address,
                        PropertyAddress::Placement {
                            property: otrios_interfaces::oib::designer::PlacementProperty::X
                                | otrios_interfaces::oib::designer::PlacementProperty::Y,
                            ..
                        } | PropertyAddress::Structural {
                            property: otrios_interfaces::oib::designer::StructuralProperty::X
                                | otrios_interfaces::oib::designer::StructuralProperty::Y,
                            ..
                        }
                    ) {
                        16000.
                    } else if matches!(self.address, PropertyAddress::Structural {
                    property: otrios_interfaces::oib::designer::StructuralProperty::Width
                        | otrios_interfaces::oib::designer::StructuralProperty::Height
                        | otrios_interfaces::oib::designer::StructuralProperty::LayoutGap
                        | otrios_interfaces::oib::designer::StructuralProperty::Padding
                        | otrios_interfaces::oib::designer::StructuralProperty::PaddingLeft
                        | otrios_interfaces::oib::designer::StructuralProperty::PaddingTop
                        | otrios_interfaces::oib::designer::StructuralProperty::PaddingRight
                        | otrios_interfaces::oib::designer::StructuralProperty::PaddingBottom
                        | otrios_interfaces::oib::designer::StructuralProperty::MinWidth
                        | otrios_interfaces::oib::designer::StructuralProperty::MinHeight
                        | otrios_interfaces::oib::designer::StructuralProperty::MaxWidth
                        | otrios_interfaces::oib::designer::StructuralProperty::MaxHeight,
                    ..
                }) {
                        8192.
                    } else {
                        1000.
                    };
                let minimum = if matches!(
                    self.address,
                    PropertyAddress::Structural {
                        property: otrios_interfaces::oib::designer::StructuralProperty::X
                            | otrios_interfaces::oib::designer::StructuralProperty::Y,
                        ..
                    }
                ) {
                    -8192.
                } else {
                    0.
                };
                if !(minimum..=maximum).contains(&n)
                    || self.descriptor.number_range.is_some_and(|[lo, hi]| {
                        !lo.is_finite() || !hi.is_finite() || n < lo || n > hi || lo > hi
                    })
                {
                    Err(Error::InvalidNumber)
                } else {
                    Ok(Value::Number(n))
                }
            }
            ValueType::Color => {
                let hex = raw(Part::Hex);
                if !matches!(hex.len(), 7 | 9)
                    || !hex.starts_with('#')
                    || !hex[1..].bytes().all(|c| c.is_ascii_hexdigit())
                {
                    return Err(Error::InvalidColor);
                }
                let mut rgba = [0, 0, 0, 255];
                for (i, channel) in rgba.iter_mut().enumerate().take((hex.len() - 1) / 2) {
                    *channel = u8::from_str_radix(&hex[1 + i * 2..3 + i * 2], 16)
                        .map_err(|_| Error::InvalidColor)?;
                }
                let opacity = parse_number(raw(Part::Opacity)).ok_or(Error::InvalidOpacity)?;
                if !(0. ..=100.).contains(&opacity) {
                    return Err(Error::InvalidOpacity);
                }
                // Eight-digit input supplies a complete RGBA value. On the next
                // opacity edit it is normalized to six digits first.
                if hex.len() == 7 {
                    rgba[3] = (opacity * 255. / 100.).round() as u8;
                }
                Ok(Value::Color(rgba))
            }
            ValueType::Boolean => Ok(Value::Boolean(self.boolean)),
            _ => Err(Error::ReadOnly),
        }
    }
    fn load(&mut self, value: &Value) {
        let values = raw_values(value);
        for field in &mut self.fields {
            if let Some((_, text)) = values.iter().find(|(part, _)| *part == field.part) {
                field.buffer.replace(text);
            }
        }
        if let Value::Boolean(value) = value {
            self.boolean = *value;
        }
    }
    fn changed(&mut self) {
        self.dirty = true;
        self.pending = None;
        self.error = self.preview().err();
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Candidate {
    pub address: PropertyAddress,
    pub value: Value,
    pub base_revision: u64,
    pub source_revision: u64,
    token: u64,
}

/// IDs are monotonic over this engine's entire lifetime, including cancellation.
/// Reserve a disjoint ID range from other tooling controls. Recreating the engine
/// requires a new range or retiring the entire old host input session.
#[derive(Clone)]
pub(crate) struct PropertyDrafts {
    drafts: Vec<Draft>,
    focus: Option<Focus>,
    next_id: u32,
    last_id: u32,
    next_token: u64,
}
impl PropertyDrafts {
    pub fn new(first_id: u32, last_id: u32) -> Self {
        Self {
            drafts: Vec::new(),
            focus: None,
            next_id: first_id,
            last_id,
            next_token: 1,
        }
    }
    pub fn drafts(&self) -> &[Draft] {
        &self.drafts
    }
    pub fn draft(&self, address: &PropertyAddress) -> Option<&Draft> {
        self.drafts.iter().find(|d| &d.address == address)
    }
    pub fn focused(&self) -> Option<&Focus> {
        self.focus.as_ref()
    }
    pub fn focused_input(&self, bounds: [f32; 4], font: &Font) -> Option<TextInput> {
        let focus = self.focus.as_ref()?;
        if !self.draft(&focus.address)?.available {
            return None;
        }
        Some(
            self.draft(&focus.address)?
                .field(focus.part)?
                .input(true, bounds, font),
        )
    }
    pub fn place_cursor(&mut self, field_id: u32, byte: usize) -> Result<bool, Error> {
        let (index, part) = self.locate(field_id)?;
        let draft = &mut self.drafts[index];
        if !draft.available {
            return Err(Error::Unavailable);
        }
        Ok(draft
            .fields
            .iter_mut()
            .find(|f| f.part == part)
            .expect("located field")
            .buffer
            .place_cursor(byte))
    }
    pub fn open(&mut self, info: &PropertyInfo, revision: u64) -> Result<&Draft, Error> {
        let address = writable(info)?.clone();
        if self.draft(&address).is_some() {
            self.refresh(&address, Some(info), revision)?;
            return Ok(self.draft(&address).expect("existing draft"));
        }
        if self.drafts.len() == MAX_DRAFTS {
            return Err(Error::Capacity);
        }
        let value = info.value.as_ref().ok_or(Error::Unavailable)?;
        let values = raw_values(value);
        let count = u32::try_from(values.len()).map_err(|_| Error::IdExhausted)?;
        // Keep the successor representable; u32::MAX is deliberately reserved.
        let next = self.next_id.checked_add(count).ok_or(Error::IdExhausted)?;
        if self.next_id == 0 || (count > 0 && next - 1 > self.last_id) {
            return Err(Error::IdExhausted);
        }
        let fields = values
            .into_iter()
            .enumerate()
            .map(|(offset, (part, text))| {
                let mut buffer = TextBuffer::with_limit(RAW_BYTES);
                buffer.replace(&text);
                Field {
                    id: self.next_id + offset as u32,
                    part,
                    buffer,
                }
            })
            .collect();
        let draft = Draft {
            address: address.clone(),
            fields,
            dirty: false,
            error: None,
            base_revision: revision,
            source_revision: revision,
            descriptor: info.descriptor.clone(),
            boolean: matches!(value, Value::Boolean(true)),
            available: true,
            pending: None,
        };
        draft.preview()?;
        self.next_id = next;
        self.drafts.push(draft);
        Ok(self.draft(&address).expect("inserted draft"))
    }
    /// Refresh from a current query, including disappearance. Dirty/focused raw
    /// input and its base marker survive remote updates and selection changes.
    pub fn refresh(
        &mut self,
        address: &PropertyAddress,
        info: Option<&PropertyInfo>,
        revision: u64,
    ) -> Result<(), Error> {
        let focused = self.focus.as_ref().is_some_and(|f| &f.address == address);
        let draft = self
            .drafts
            .iter_mut()
            .find(|d| &d.address == address)
            .ok_or(Error::Unavailable)?;
        draft.source_revision = revision;
        let valid = info.ok_or(Error::Unavailable).and_then(|info| {
            if writable(info)? != address {
                return Err(Error::Unavailable);
            }
            if info.descriptor.value_type != draft.descriptor.value_type {
                return Err(Error::Unavailable);
            }
            Ok(info)
        });
        match valid {
            Err(error) => {
                draft.available = false;
                draft.error = Some(error);
                draft.pending = None;
                Err(error)
            }
            Ok(info) => {
                draft.available = true;
                draft.descriptor = info.descriptor.clone();
                if !draft.dirty && !focused {
                    draft.load(info.value.as_ref().ok_or(Error::Unavailable)?);
                    draft.base_revision = revision;
                }
                draft.error = draft.preview().err();
                Ok(())
            }
        }
    }
    pub fn focus(&mut self, field_id: u32) -> Result<(), Error> {
        let (draft, part) = self.locate(field_id)?;
        if !self.drafts[draft].available {
            return Err(Error::Unavailable);
        }
        self.focus = Some(Focus {
            address: self.drafts[draft].address.clone(),
            part,
        });
        Ok(())
    }
    /// Blur does not implicitly discard invalid text. Call prepare using the
    /// captured original address before clearing focus; errors stay on that draft.
    pub fn clear_focus(&mut self) {
        self.focus = None;
    }
    fn locate(&self, id: u32) -> Result<(usize, Part), Error> {
        self.drafts
            .iter()
            .enumerate()
            .find_map(|(i, d)| d.fields.iter().find(|f| f.id == id).map(|f| (i, f.part)))
            .ok_or(Error::UnknownField)
    }
    pub fn edit(&mut self, field_id: u32, edit: &Edit) -> Result<EditResult, Error> {
        let (index, part) = self.locate(field_id)?;
        let draft = &mut self.drafts[index];
        if !draft.available {
            return Err(draft.error.unwrap_or(Error::Unavailable));
        }
        match edit {
            Edit::Enter => return Ok(EditResult::CommitRequested),
            Edit::Tab { backward } => {
                return Ok(EditResult::Traverse {
                    backward: *backward,
                });
            }
            _ => {}
        }
        // Preserve the alpha encoded in eight-digit hex when opacity becomes
        // the edited part. Do not normalize on caret-only navigation.
        let mut buffer = draft.field(part).expect("located field").buffer.clone();
        let old = buffer.text().to_owned();
        if matches!(edit, Edit::Replace { text, .. } | Edit::Commit(text) if text.len() > RAW_BYTES)
        {
            draft.error = Some(Error::RawLimit);
            return Err(Error::RawLimit);
        }
        let changed = buffer.edit(edit);
        let text_changed = old != buffer.text();
        if text_changed
            && part == Part::Opacity
            && let Some(hex) = draft.fields.iter_mut().find(|f| f.part == Part::Hex)
            && hex.buffer.text().len() == 9
            && hex.buffer.text().is_ascii()
        {
            let rgb = hex.buffer.text()[..7].to_owned();
            hex.buffer.replace(&rgb);
        }
        draft
            .fields
            .iter_mut()
            .find(|f| f.part == part)
            .expect("located field")
            .buffer = buffer;
        if text_changed && part == Part::Hex {
            // Keep the separately visible opacity consistent with pasted RGBA,
            // including recovery from a previously invalid opacity draft.
            let hex = draft.field(Part::Hex).expect("color field").buffer.text();
            if hex.len() == 9
                && hex.starts_with('#')
                && hex[1..].bytes().all(|c| c.is_ascii_hexdigit())
            {
                let alpha = u8::from_str_radix(&hex[7..9], 16).map_err(|_| Error::InvalidColor)?;
                let opacity = (f32::from(alpha) * 100. / 255.).to_string();
                if let Some(field) = draft.fields.iter_mut().find(|f| f.part == Part::Opacity) {
                    field.buffer.replace(&opacity);
                }
            }
        }
        if text_changed {
            draft.changed();
        }
        Ok(EditResult::Local(changed))
    }
    pub fn set_boolean(&mut self, address: &PropertyAddress, value: bool) -> Result<(), Error> {
        let draft = self.mutable(address, ValueType::Boolean)?;
        draft.boolean = value;
        draft.changed();
        Ok(())
    }
    /// Picker motion only changes local buffers. Release/Apply calls prepare.
    pub fn set_color(&mut self, address: &PropertyAddress, rgba: [u8; 4]) -> Result<(), Error> {
        let draft = self.mutable(address, ValueType::Color)?;
        draft.load(&Value::Color(rgba));
        draft.changed();
        Ok(())
    }
    pub fn set_opacity(&mut self, address: &PropertyAddress, percent: f32) -> Result<(), Error> {
        if !percent.is_finite()
            || !(0. ..=100.).contains(&percent)
            || (percent == 0. && percent.is_sign_negative())
        {
            return Err(Error::InvalidOpacity);
        }
        let draft = self.mutable(address, ValueType::Color)?;
        let Value::Color(mut rgba) = draft.preview()? else {
            return Err(Error::InvalidColor);
        };
        rgba[3] = (percent * 255. / 100.).round() as u8;
        draft.load(&Value::Color(rgba));
        draft.changed();
        Ok(())
    }
    fn mutable(&mut self, address: &PropertyAddress, kind: ValueType) -> Result<&mut Draft, Error> {
        let draft = self
            .drafts
            .iter_mut()
            .find(|d| &d.address == address)
            .ok_or(Error::Unavailable)?;
        if !draft.available {
            return Err(Error::Unavailable);
        }
        if draft.descriptor.value_type != kind {
            return Err(Error::ReadOnly);
        }
        Ok(draft)
    }
    /// Always permits an explicit equal-value assignment. This is a candidate,
    /// never a causal write or proof that the fresh authority permits the write.
    pub fn prepare(
        &mut self,
        address: &PropertyAddress,
        fresh: &PropertyInfo,
        revision: u64,
    ) -> Result<Candidate, Error> {
        self.refresh(address, Some(fresh), revision)?;
        let draft = self
            .drafts
            .iter_mut()
            .find(|d| &d.address == address)
            .ok_or(Error::Unavailable)?;
        let value = draft.preview().inspect_err(|error| {
            draft.error = Some(*error);
        })?;
        let token = self.next_token;
        self.next_token = token.checked_add(1).ok_or(Error::IdExhausted)?;
        let candidate = Candidate {
            address: address.clone(),
            value,
            base_revision: draft.base_revision,
            source_revision: revision,
            token,
        };
        draft.pending = Some(candidate.clone());
        Ok(candidate)
    }
    pub fn acknowledge(&mut self, candidate: &Candidate, accepted: bool) -> Result<(), Error> {
        self.acknowledge_at(candidate, accepted, candidate.source_revision)
    }
    /// A private staging transaction can refresh the source before its field is
    /// acknowledged. Keep the token check, then bind accepted input to that
    /// refreshed source so the next edit in the same focused field is current.
    pub fn acknowledge_at(
        &mut self,
        candidate: &Candidate,
        accepted: bool,
        current_revision: u64,
    ) -> Result<(), Error> {
        let draft = self
            .drafts
            .iter_mut()
            .find(|d| d.address == candidate.address)
            .ok_or(Error::StaleCandidate)?;
        if draft.pending.as_ref() != Some(candidate) {
            return Err(Error::StaleCandidate);
        }
        draft.pending = None;
        if accepted {
            draft.load(&candidate.value);
            draft.dirty = false;
            draft.error = None;
            draft.base_revision = current_revision;
            draft.source_revision = current_revision;
        }
        Ok(())
    }
    /// Explicit discard also retires every field ID. Nothing is emitted.
    pub fn cancel(&mut self, address: &PropertyAddress) -> bool {
        if self.focus.as_ref().is_some_and(|f| &f.address == address) {
            self.focus = None;
        }
        let old = self.drafts.len();
        self.drafts.retain(|d| &d.address != address);
        old != self.drafts.len()
    }
}
fn writable(info: &PropertyInfo) -> Result<&PropertyAddress, Error> {
    let address = info.address.as_ref().ok_or(Error::Unavailable)?;
    if (info.cell.is_none()
        && !matches!(
            address,
            PropertyAddress::Placement { .. } | PropertyAddress::Structural { .. }
        ))
        || info.source == "bound"
        || !info.descriptor.policy.live_literal
    {
        return Err(Error::ReadOnly);
    }
    let allowed = matches!(
        (info.descriptor.editor, info.descriptor.value_type),
        (PropertyEditor::TextSuggestions, ValueType::Text)
            | (PropertyEditor::NumberSuggestions, ValueType::Number)
            | (PropertyEditor::Palette, ValueType::Color)
            | (PropertyEditor::Toggle, ValueType::Boolean)
    );
    if !allowed {
        return Err(Error::ReadOnly);
    }
    let value = info.value.as_ref().ok_or(Error::Unavailable)?;
    let valid = matches!((info.descriptor.value_type, value),
        (ValueType::Text, Value::Text(text)) if !text.is_empty() && text.len() <= 96 && !text.contains('\0'))
        || matches!(
            (info.descriptor.value_type, value),
            (ValueType::Number, Value::Number(_))
                | (ValueType::Color, Value::Color(_))
                | (ValueType::Boolean, Value::Boolean(_))
        );
    if !valid {
        return Err(Error::Unavailable);
    }
    Ok(address)
}
fn parse_number(raw: &str) -> Option<f32> {
    let n: f32 = raw.parse().ok()?;
    (n.is_finite() && !(n == 0. && n.is_sign_negative())).then_some(n)
}
fn raw_values(value: &Value) -> Vec<(Part, String)> {
    match value {
        Value::Text(text) => vec![(Part::Value, text.clone())],
        Value::Number(n) => vec![(Part::Value, n.to_string())],
        Value::Color([r, g, b, a]) => vec![
            (Part::Hex, format!("#{r:02X}{g:02X}{b:02X}")),
            (Part::Opacity, (f32::from(*a) * 100. / 255.).to_string()),
        ],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use otrios_interfaces::oib::designer::{
        InspectorGroup, InstanceId, NodeId, PropertyId, PropertyPath, PropertyPolicy,
    };
    #[test]
    fn acknowledged_local_revision_supports_repeated_edits_in_focused_field() {
        let address = PropertyAddress::Occurrence {
            instance: InstanceId(1),
            path: PropertyPath {
                references: vec![],
                node: NodeId(1),
                property: PropertyId(1),
            },
        };
        let mut info = PropertyInfo {
            address: Some(address.clone()),
            cell: Some(1),
            descriptor: PropertyDescriptor {
                id: PropertyId(1),
                name: "width".into(),
                label: "Width".into(),
                value_type: ValueType::Number,
                default: Value::Number(10.),
                number_range: None,
                number_step: None,
                units: None,
                group: InspectorGroup::Appearance,
                editor: PropertyEditor::NumberSuggestions,
                policy: PropertyPolicy {
                    live_literal: true,
                    app_binding: false,
                },
            },
            value: Some(Value::Number(10.)),
            inherited: None,
            origin: "component".into(),
            source: "literal".into(),
            overridden: false,
        };
        let mut drafts = PropertyDrafts::new(100, 200);
        let field = drafts.open(&info, 1).unwrap().fields[0].id;
        drafts.focus(field).unwrap();
        for (revision, value) in [(1, "20"), (2, "30")] {
            drafts
                .edit(
                    field,
                    &Edit::Replace {
                        text: value.into(),
                        cursor: 2,
                        anchor: 2,
                        selection_only: false,
                    },
                )
                .unwrap();
            let candidate = drafts.prepare(&address, &info, revision).unwrap();
            assert_eq!(candidate.base_revision, revision);
            info.value = Some(candidate.value.clone());
            // Working-copy refresh occurs while the raw field is still dirty.
            drafts.refresh(&address, Some(&info), revision + 1).unwrap();
            drafts
                .acknowledge_at(&candidate, true, revision + 1)
                .unwrap();
            assert_eq!(drafts.focused().unwrap().address, address);
            assert_eq!(drafts.draft(&address).unwrap().base_revision, revision + 1);
            assert!(!drafts.draft(&address).unwrap().dirty);
            assert_eq!(
                drafts.acknowledge_at(&candidate, true, revision + 2),
                Err(Error::StaleCandidate)
            );
        }
    }
}
