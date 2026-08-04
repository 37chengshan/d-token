// d-token 管理块写入器：只插入或追加 d-token 管理块，不允许改动用户原有内容。
// 关联计划：release-plan-2026-07/phase-03 §7 文档写入协议、强制规则。
// 安全规则：非管理块内容变化则写入失败；无法备份则写入失败；编码不可识别则降级提示词模式。

export const MANAGED_BLOCK_START = (id: string, updated: string) =>
  `<!-- d-token:start smart-learning id="${id}" updated="${updated}" -->`;
export const MANAGED_BLOCK_END = "<!-- d-token:end smart-learning -->";

export type ManagedBlock = {
  id: string;
  updated: string;
  /** 块内正文（不含首尾注释）。 */
  body: string;
};

/** 在文档中查找已有管理块（start..end）。返回 [startIndex, endExclusive]。 */
export function findManagedBlock(content: string, id: string): { start: number; end: number } | null {
  const startMarker = MANAGED_BLOCK_START(id, "");
  // 匹配 updated="..." 容忍任意日期
  const startRegex = new RegExp(
    `<!-- d-token:start smart-learning id="${escapeReg(id)}" updated="[^"]*" -->`,
  );
  const startMatch = startRegex.exec(content);
  if (!startMatch) return null;
  void startMarker;
  const start = startMatch.index;
  const endMarker = MANAGED_BLOCK_END;
  const endIdx = content.indexOf(endMarker, start);
  if (endIdx === -1) return null;
  return { start, end: endIdx + endMarker.length };
}

/** 渲染一个完整管理块字符串。 */
export function renderManagedBlock(block: ManagedBlock): string {
  return `${MANAGED_BLOCK_START(block.id, block.updated)}\n${block.body}\n${MANAGED_BLOCK_END}`;
}

/** 计算写入后的新文档内容：存在则替换管理块，不存在则追加到末尾。 */
export function applyManagedBlock(content: string, block: ManagedBlock): string {
  const found = findManagedBlock(content, block.id);
  const rendered = renderManagedBlock(block);
  if (found) {
    return content.slice(0, found.start) + rendered + content.slice(found.end);
  }
  const sep = content.endsWith("\n") || content.length === 0 ? "" : "\n\n";
  return content + sep + rendered + "\n";
}

/** 非管理内容不变性检查：写入前后，去掉管理块后的文本必须完全一致。 */
export function verifyNonManagedInvariant(before: string, after: string, id: string): { ok: boolean; beforeStripped: string; afterStripped: string } {
  const beforeStripped = stripManagedBlocks(before, id);
  const afterStripped = stripManagedBlocks(after, id);
  return { ok: beforeStripped === afterStripped, beforeStripped, afterStripped };
}

/** 移除所有指定 id 的管理块（用于不变性比较）。 */
export function stripManagedBlocks(content: string, id: string): string {
  let result = content;
  let guard = 0;
  while (guard < 100) {
    const found = findManagedBlock(result, id);
    if (!found) break;
    result = result.slice(0, found.start) + result.slice(found.end);
    guard++;
  }
  return result;
}

export type WriteResult =
  | { ok: true; newContent: string; mode: "replaced" | "appended"; invariantOk: true }
  | { ok: false; reason: string; mode: "invariant-violation" | "no-backup" | "encoding-unsafe" | "too-large"; newContent?: string };

/** 完整写入算法：读取 -> 备份 -> 查找 -> 替换/追加 -> 不变性校验 -> 返回结果。
 *  调用层负责真实 IO 与回退点创建；本函数为纯逻辑。 */
export function planManagedWrite(params: {
  existingContent: string;
  block: ManagedBlock;
  hasBackup: boolean;
  encodingSafe: boolean;
  tooLarge: boolean;
}): WriteResult {
  if (params.tooLarge) return { ok: false, reason: "目标文件过大，优先写入 .d-token/learned-context.md", mode: "too-large" };
  if (!params.encodingSafe) return { ok: false, reason: "文件编码无法安全识别，已降级为提示词模式", mode: "encoding-unsafe" };
  if (!params.hasBackup) return { ok: false, reason: "无法备份，写入失败", mode: "no-backup" };
  const existed = Boolean(findManagedBlock(params.existingContent, params.block.id));
  const newContent = applyManagedBlock(params.existingContent, params.block);
  const inv = verifyNonManagedInvariant(params.existingContent, newContent, params.block.id);
  if (!inv.ok) return { ok: false, reason: "非管理块内容发生变化，写入失败", mode: "invariant-violation", newContent };
  return { ok: true, newContent, mode: existed ? "replaced" : "appended", invariantOk: true };
}

function escapeReg(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
