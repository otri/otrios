;; SPDX-License-Identifier: Apache-2.0
(module
  (global $step i32 (i32.const 1))
  (func (export "abi_version") (result i32) i32.const 1)
  (func (export "dispatch") (param $event i32) (param i32) (param i32) (param $current i32) (result i64)
    (if (result i64) (i32.eq (local.get $event) (i32.const 1))
      (then (i64.or (i64.const 1125899906842624)
        (i64.extend_i32_u (i32.rem_u (i32.add (local.get $current) (global.get $step)) (i32.const 1000)))))
      (else (if (result i64) (i32.eq (local.get $event) (i32.const 2))
        (then (i64.const 1125899906842624))
        (else (i64.const 0)))))))
