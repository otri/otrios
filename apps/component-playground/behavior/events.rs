// SPDX-License-Identifier: Apache-2.0
//! Stable v1 guest event ABI, shared by UI handlers and the WAT dispatch contract.
use crate::authoring::BehaviorEvent;
pub const SET_LEVEL: BehaviorEvent = BehaviorEvent(1);
pub const SET_ANGLE: BehaviorEvent = BehaviorEvent(2);
pub const SET_PAD: BehaviorEvent = BehaviorEvent(3);
pub const TOGGLE: BehaviorEvent = BehaviorEvent(5);
