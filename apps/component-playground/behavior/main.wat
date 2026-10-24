;; SPDX-License-Identifier: Apache-2.0
;; No imports. Each bit in the command mask authorizes one populated ten-bit lane.
(module
  (func (export "abi_version") (result i32) i32.const 1)
  (func (export "dispatch") (param $event i32) (param $a i32) (param $b i32) (param $current i32) (result i64)
    local.get $event i32.const 1 i32.eq
    if (result i64)
      ;; linear-value-begin
      ;; Quantize the linear control locally; the host and UI layout are unchanged.
      local.get $a i32.const 100 i32.div_u i32.const 100 i32.mul
      ;; linear-value-end
      i64.extend_i32_u i64.const 1125899906842624 i64.or
    else
      local.get $event i32.const 2 i32.eq
      if (result i64)
        local.get $a i64.extend_i32_u i64.const 10 i64.shl i64.const 2251799813685248 i64.or
      else
        local.get $event i32.const 3 i32.eq
        if (result i64)
          local.get $a i64.extend_i32_u i64.const 20 i64.shl
          local.get $b i64.extend_i32_u i64.const 30 i64.shl i64.or
          i64.const 13510798882111488 i64.or
        else
          local.get $event i32.const 5 i32.eq
          if (result i64)
            i32.const 1000 local.get $current i32.sub i64.extend_i32_u i64.const 40 i64.shl
            i64.const 18014398509481984 i64.or
          else unreachable end
        end
      end
    end))
