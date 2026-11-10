;; SPDX-License-Identifier: Apache-2.0
;; Library-owned numeric drag policy. Changing STEP changes guest bytes only.
(module
  (memory (export "memory") 1 1)
  (func (export "abi_version") (result i32) i32.const 1)
  (func (export "input_buffer") (result i32) i32.const 0)
  (data (i32.const 8192) "\4f\54\43\42\4f\55\54\31\01\00\00\00\00\00\00\00\02\00\00\00\00\00\00\00\01\00\00\00\00\00\00\00\01\00\04\00\00\00\00\00")
  (data (i32.const 8256) "\4f\54\43\42\4f\55\54\31\00\00\00\00\00\00\00\00")
  (func $step (result i32) (i32.const 1)) ;; STEP
  (func (export "dispatch") (param $input i32) (param $length i32) (result i64)
    (local $value i32) (local $step i32)
    ;; Behavior identity is library-declared, not an app field number.
    (if (i64.ne (i64.load offset=40 (local.get $input)) (i64.const 1))
      (then unreachable))
    ;; Cancel does not emit an app effect.
    (if (i32.eq (i32.load8_u offset=8 (local.get $input)) (i32.const 3))
      (then (return (i64.const 68719484992))))
    (local.set $step (call $step))
    (local.set $value (i32.load16_u offset=10 (local.get $input)))
    (local.set $value
      (i32.mul
        (i32.div_u (i32.add (local.get $value) (i32.div_u (local.get $step) (i32.const 2))) (local.get $step))
        (local.get $step)))
    (f32.store (i32.const 8228) (f32.convert_i32_u (local.get $value)))
    ;; Low32 is the output offset, high32 its exact canonical byte length.
    (i64.const 171798700032)))
