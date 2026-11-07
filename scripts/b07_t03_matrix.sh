#!/usr/bin/env bash
# B07-T03 八类候选 + 边界的一次性驱动：只用 CLI 公开动词，逐步记录退出码与输出。
# 用法：scripts/b07_t03_matrix.sh <CLI 二进制> <工作目录>
set -u

CLI="${1:?需要 CLI 二进制路径}"
WORK="${2:?需要工作目录}"
PKG="$(cd "$(dirname "$0")/.." && pwd)/fixtures/managed-vault-update-v1"
CASES=(edit-body add-link replace-attachment rename-linked edit-after-approval rename-dangling delete-note foreign-touch)

rm -rf "$WORK"
mkdir -p "$WORK"

fail=0
note() { printf '%s\n' "$*"; }
expect_exit() { # expect_exit <want> <label> <cmd...>
  local want="$1" label="$2"; shift 2
  local out; out="$("$@" 2>&1)"; local code=$?
  if [ "$code" != "$want" ]; then
    note "[失败] $label 期望 exit $want，实得 $code"
    note "$out"
    fail=$((fail + 1))
  else
    note "[通过] $label exit $code"
  fi
  printf '%s' "$out" > "$WORK/.last"
}

# ---------- 1. 陌生 Vault 只读 ----------
mkdir -p "$WORK/stranger/笔记"
printf '人工内容\n' > "$WORK/stranger/笔记/我写的.md"
mkdir -p "$WORK/stranger/.obsidian"
printf '{"vaultName":"陌生"}\n' > "$WORK/stranger/.obsidian/app.json"
stranger_before="$(cd "$WORK/stranger" && find . -type f -exec sha256sum {} + | sort)"
stranger_out="$("$CLI" vault adopt "$PKG/author.json" "$WORK/stranger" 2>&1)"; stranger_code=$?
# 契约要求「陌生 Vault 一律拒绝接管，什么都不改」；adopt 的根必须尚不存在，
# 因此拒绝发生在参数层（exit 2 / VAULT_ARGUMENT），不是内容判定层。
if [ "$stranger_code" != "0" ] && printf '%s' "$stranger_out" | grep -q 'VAULT_ARGUMENT'; then
  note "[通过] 陌生 Vault adopt 被拒绝 exit $stranger_code（VAULT_ARGUMENT）"
else
  note "[失败] 陌生 Vault adopt 应被拒绝，实得 exit $stranger_code"
  note "$stranger_out"
  fail=$((fail + 1))
fi
stranger_after="$(cd "$WORK/stranger" && find . -type f -exec sha256sum {} + | sort)"
if [ "$stranger_before" = "$stranger_after" ]; then
  note "[通过] 陌生 Vault 全部字节未变"
else
  note "[失败] 陌生 Vault 字节被改动"; fail=$((fail + 1))
fi

# ---------- 2. adopt ----------
expect_exit 0 "adopt" "$CLI" vault adopt "$PKG/author.json" "$WORK/root"
# 无关内容放进 vault/ 之外的地面：foreign/ 由人工拷进托管根外层的同名目录不参与
before_foreign="$(cd "$PKG/foreign" && find . -type f -exec sha256sum {} + | sort)"
expect_exit 0 "status（未发布）" "$CLI" vault status "$WORK/root"
grep -q '待恢复：否' "$WORK/.last" || { note "[失败] status 未报告待恢复：否"; fail=$((fail + 1)); }
grep -q '未批准覆盖：0' "$WORK/.last" || { note "[失败] status 未报告未批准覆盖 0"; fail=$((fail + 1)); }

# ---------- 3. 八类候选 ----------
for case in "${CASES[@]}"; do
  cand="$PKG/candidates/$case"
  want="$(python3 - "$PKG/cases.json" "$case" <<'PY'
import json,sys
data=json.load(open(sys.argv[1]))
for c in data["cases"]:
    if c["id"]==sys.argv[2]:
        print(c["expect"]["decision"], c["expect"]["refusal"] or "-")
PY
)"
  decision="${want%% *}"; refusal="${want##* }"
  if [ "$decision" = "refuse" ]; then
    expect_exit 3 "plan $case → $refusal" "$CLI" vault plan "$WORK/root" "$cand" "$WORK/$case.plan.json"
    grep -q "$refusal" "$WORK/.last" || { note "[失败] $case 未给出 $refusal"; fail=$((fail + 1)); }
    [ -e "$WORK/$case.plan.json" ] && { note "[失败] $case 拒绝了却写出计划文件"; fail=$((fail + 1)); }
  else
    expect_exit 0 "plan $case" "$CLI" vault plan "$WORK/root" "$cand" "$WORK/$case.plan.json"
  fi
done

# ---------- 4. 批准后人工编辑令批准失效 ----------
expect_exit 0 "plan edit-after-approval 第二次" "$CLI" vault plan "$WORK/root" "$PKG/candidates/edit-after-approval" "$WORK/stale.plan.json"
expect_exit 0 "approve stale.plan" "$CLI" vault approve "$WORK/root" "$WORK/stale.plan.json" "$WORK/stale.approval.json"
printf '人工改写\n' > "$WORK/root/vault/笔记/力学.md"
expect_exit 3 "publish 过期批准被拒绝" "$CLI" vault publish "$WORK/root" "$WORK/stale.approval.json"
grep -q 'approval-stale' "$WORK/.last" || { note "[失败] 未报告 approval-stale"; fail=$((fail + 1)); }
if [ "$(cat "$WORK/root/vault/笔记/力学.md")" = "人工改写" ]; then
  note "[通过] 过期批准后 vault/ 保持人工字节"
else
  note "[失败] 过期批准的 publish 改动了 vault/"; fail=$((fail + 1))
fi
# 还原成基准字节，好让后续 case 从一致状态开始
cp "$WORK/root/baseline/笔记/力学.md" "$WORK/root/vault/笔记/力学.md"

# ---------- 5. 正常发布 + 独立检查 ----------
expect_exit 0 "plan edit-body" "$CLI" vault plan "$WORK/root" "$PKG/candidates/edit-body" "$WORK/ok.plan.json"
expect_exit 0 "approve ok.plan" "$CLI" vault approve "$WORK/root" "$WORK/ok.plan.json" "$WORK/ok.approval.json"
expect_exit 0 "publish ok" "$CLI" vault publish "$WORK/root" "$WORK/ok.approval.json"
grep -q 'status=published' "$WORK/.last" || { note "[失败] publish 未报告 status=published"; fail=$((fail + 1)); }
expect_exit 0 "独立检查器核验已发布根" python3 "$(dirname "$0")/verify_managed_vault.py" "$WORK/root" --package "$PKG" --run-dir "$WORK"

# ---------- 6. 同一批准重放幂等 ----------
expect_exit 0 "publish 重放" "$CLI" vault publish "$WORK/root" "$WORK/ok.approval.json"
before_replay="$(cd "$WORK/root" && find vault -type f -exec sha256sum {} + | sort)"
expect_exit 0 "publish 再次重放" "$CLI" vault publish "$WORK/root" "$WORK/ok.approval.json"
after_replay="$(cd "$WORK/root" && find vault -type f -exec sha256sum {} + | sort)"
if [ "$before_replay" = "$after_replay" ]; then
  note "[通过] 重放未改写 vault/"
else
  note "[失败] 重放改写了 vault/"; fail=$((fail + 1))
fi

# ---------- 7. rollback 往返 ----------
expect_exit 0 "rollback 生成反向计划" "$CLI" vault rollback "$WORK/root" "$WORK/rb.plan.json"
expect_exit 0 "approve 反向计划" "$CLI" vault approve "$WORK/root" "$WORK/rb.plan.json" "$WORK/rb.approval.json"
expect_exit 0 "publish 反向计划" "$CLI" vault publish "$WORK/root" "$WORK/rb.approval.json"
if cmp -s "$WORK/root/vault/笔记/力学.md" "$WORK/root/baseline/笔记/力学.md"; then
  note "[通过] rollback 后与基准一致"
else
  note "[失败] rollback 后与基准不一致"; fail=$((fail + 1))
fi
expect_exit 0 "独立检查器核验回滚后根" python3 "$(dirname "$0")/verify_managed_vault.py" "$WORK/root" --package "$PKG" --run-dir "$WORK"

# ---------- 8. 回滚冲突：发布后人工改动 ----------
expect_exit 0 "plan edit-body 第三轮" "$CLI" vault plan "$WORK/root" "$PKG/candidates/edit-body" "$WORK/c.plan.json"
expect_exit 0 "approve c.plan" "$CLI" vault approve "$WORK/root" "$WORK/c.plan.json" "$WORK/c.approval.json"
expect_exit 0 "publish c" "$CLI" vault publish "$WORK/root" "$WORK/c.approval.json"
printf '发布之后人工又改了一次\n' > "$WORK/root/vault/笔记/力学.md"
rb2_out="$("$CLI" vault rollback "$WORK/root" "$WORK/rb2.plan.json" 2>&1)"; rb2_code=$?
if [ "$rb2_code" = "3" ] && printf '%s' "$rb2_out" | grep -q 'rollback-conflict'; then
  note "[通过] rollback 遇后续人工改动 exit 3（rollback-conflict）"
else
  note "[失败] rollback 冲突期望 exit 3 + rollback-conflict，实得 exit $rb2_code"
  note "$rb2_out"
  fail=$((fail + 1))
fi
[ -e "$WORK/rb2.plan.json" ] && { note "[失败] 冲突时仍写出回滚计划"; fail=$((fail + 1)); }

# ---------- 9. 真实 SIGKILL 与恢复 ----------
# pending 记录落账、vault/ 尚未开始逐项写盘时杀掉进程；status 必须报 needs_recovery，
# 同一批准重跑必须能续上，并由独立检查器核对最终一致。
expect_exit 0 "plan add-link（用于中断）" "$CLI" vault plan "$WORK/root" "$PKG/candidates/add-link" "$WORK/k.plan.json"
expect_exit 0 "approve k.plan" "$CLI" vault approve "$WORK/root" "$WORK/k.plan.json" "$WORK/k.approval.json"
NOETOME_VAULT_TEST_PAUSE_AFTER_PENDING=1 "$CLI" vault publish "$WORK/root" "$WORK/k.approval.json" > "$WORK/pause.out" 2>&1 &
pause_pid=$!
ready=0
for _ in $(seq 1 100); do
  if grep -q NOETOME_VAULT_TEST_READY "$WORK/pause.out" 2>/dev/null; then ready=1; break; fi
  sleep 0.1
done
if [ "$ready" = "1" ]; then
  note "[通过] publish 已到 pending 暂停点"
else
  note "[失败] publish 未到 pending 暂停点"; fail=$((fail + 1))
fi
kill -9 "$pause_pid" 2>/dev/null
wait "$pause_pid" 2>/dev/null
expect_exit 0 "status（中断后）" "$CLI" vault status "$WORK/root"
if grep -q 'needs_recovery' "$WORK/.last"; then
  note "[通过] status 报告 needs_recovery"
else
  note "[失败] status 未报告 needs_recovery"; note "$(cat "$WORK/.last")"; fail=$((fail + 1))
fi
expect_exit 0 "publish 续跑" "$CLI" vault publish "$WORK/root" "$WORK/k.approval.json"
expect_exit 0 "独立检查器核验恢复后根" python3 "$(dirname "$0")/verify_managed_vault.py" "$WORK/root" --package "$PKG" --run-dir "$WORK"
grep -q '合计 .*失败 0' "$WORK/.last" || { note "[失败] 恢复后独立检查未全通过"; fail=$((fail + 1)); }

# ---------- 10. 无关内容与 .obsidian 保持原字节 ----------
after_foreign="$(cd "$PKG/foreign" && find . -type f -exec sha256sum {} + | sort)"
if [ "$before_foreign" = "$after_foreign" ]; then
  note "[通过] 作者包 foreign/ 全部字节未变"
else
  note "[失败] 作者包 foreign/ 字节被改动"; fail=$((fail + 1))
fi
stray="$(cd "$WORK/root" && find vault -name '.obsidian' -o -name '陌生.md' -o -name '无关.png' | wc -l)"
if [ "$stray" = "0" ]; then
  note "[通过] 托管 vault/ 未混入陌生内容"
else
  note "[失败] 托管 vault/ 出现陌生条目"; fail=$((fail + 1))
fi

note
if [ "$fail" = "0" ]; then
  note "矩阵全部通过"
else
  note "矩阵失败 $fail 项"
fi
exit $(( fail > 0 ))
