// SPDX-License-Identifier: LGPL-3.0-only
//! Exact typed serialization inputs. PartialEq alone conflates signed zero.
use super::*;

pub(super) fn same(left: &Blueprint, right: &Blueprint) -> bool {
    // Layout admission excludes nonfinite floats. Every equal finite nonzero
    // f32 has the same bits; only the sign of zero needs a separate comparison.
    left == right && signs(left) == signs(right)
}

#[derive(Default)]
struct Signs {
    position: usize,
    negative_zero: Vec<usize>,
}
impl Signs {
    fn number(&mut self, value: f32) {
        if value.to_bits() == (-0.0f32).to_bits() {
            self.negative_zero.push(self.position);
        }
        self.position += 1;
    }
    fn numbers(&mut self, values: &[f32]) {
        for value in values {
            self.number(*value);
        }
    }
    fn value(&mut self, value: &Value) {
        match value {
            Value::Number(value) => self.number(*value),
            Value::Series(values) => self.numbers(values),
            Value::TimedSeries(values) => {
                for value in values {
                    self.numbers(value);
                }
            }
            Value::Boolean(_) | Value::Text(_) | Value::Color(_) => {}
        }
    }
    fn expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Literal(value) => self.value(value),
            Expr::Add(a, b) | Expr::Subtract(a, b) | Expr::Multiply(a, b) => {
                self.expr(a);
                self.expr(b);
            }
            Expr::Select { condition, yes, no } => {
                self.expr(condition);
                self.expr(yes);
                self.expr(no);
            }
            Expr::Parameter(_) | Expr::State(_) | Expr::Width | Expr::Height => {}
        }
    }
    fn axis(&mut self, axis: &AxisResize) {
        match axis {
            AxisResize::Pins { leading, trailing } => {
                for value in [leading, trailing].into_iter().flatten() {
                    self.number(*value);
                }
            }
            AxisResize::Center { offset } => self.number(*offset),
            AxisResize::Fixed
            | AxisResize::Springs {
                leading: _,
                length: _,
                trailing: _,
            } => {}
        }
    }
    fn node(&mut self, node: &TemplateNode) {
        let TemplateNode {
            id: _,
            resize,
            frame,
            visible,
            shape,
            responder: _,
            semantics,
            children,
        } = node;
        if let Some(ResizeRule {
            reference_size,
            horizontal,
            vertical,
        }) = resize
        {
            self.numbers(reference_size);
            self.axis(horizontal);
            self.axis(vertical);
        }
        for expr in frame {
            self.expr(expr);
        }
        self.expr(visible);
        match shape {
            Shape::Canvas { clip: _ } | Shape::Slot => {}
            Shape::Text {
                value,
                scale,
                color,
            } => {
                self.expr(value);
                self.expr(scale);
                self.expr(color);
            }
            Shape::Superellipse {
                radius,
                exponent,
                color,
            } => {
                self.expr(radius);
                self.number(*exponent);
                self.expr(color);
            }
            Shape::Circle { color } => self.expr(color),
            Shape::SeriesPath {
                series,
                width,
                color,
            }
            | Shape::TimedSeriesPath {
                series,
                width,
                color,
            } => {
                self.expr(series);
                self.number(*width);
                self.expr(color);
            }
        }
        if let Some(Semantics { label, checked }) = semantics {
            self.expr(label);
            self.expr(checked);
        }
        for child in children {
            self.node(child);
        }
    }
    fn instance(&mut self, instance: &Instance) {
        let Instance {
            id: _,
            template: _,
            frame,
            properties,
            children,
        } = instance;
        self.numbers(frame);
        for Property { key: _, value } in properties {
            self.expr(value);
        }
        for child in children {
            self.instance(child);
        }
    }
    fn flow(&mut self, flow: &Flow) {
        let Flow { flexible: _, kind } = flow;
        match kind {
            FlowKind::Wrap {
                nodes: _,
                preferred,
                minimum,
                gap,
            } => {
                self.numbers(preferred);
                self.numbers(minimum);
                self.numbers(gap);
            }
            FlowKind::Item {
                node: _,
                preferred,
                minimum,
            } => {
                self.numbers(preferred);
                self.numbers(minimum);
            }
            FlowKind::Group {
                axis: _,
                gap,
                alignment: _,
                children,
            } => {
                self.number(*gap);
                for child in children {
                    self.flow(child);
                }
            }
        }
    }
}
fn signs(blueprint: &Blueprint) -> Vec<usize> {
    let Blueprint {
        schema: _,
        composition,
        templates,
        root,
    } = blueprint;
    let mut signs = Signs::default();
    if let Some(Composition {
        wide_min_width,
        wide,
        tall,
    }) = composition
    {
        signs.number(*wide_min_width);
        for FlowPlan { insets, content } in [wide, tall] {
            signs.numbers(insets);
            signs.flow(content);
        }
    }
    for Template {
        id: _,
        name: _,
        parameters,
        root,
    } in templates
    {
        for Parameter {
            key: _,
            name: _,
            value_type: _,
            default,
        } in parameters
        {
            signs.expr(default);
        }
        signs.node(root);
    }
    signs.instance(root);
    signs.negative_zero
}
