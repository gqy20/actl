import type { Snapshot } from './model.ts';

const common: Snapshot = {
  version: 1, title: '整理会议记录', status: '已暂停', tone: 'waiting',
  body: '1 / 2 步完成\n写入纪要\n你已请求暂停。',
  records: '1. 读取记录 · 已完成 · 有核验记录\n2. 写入纪要 · 待执行',
  diagnostics: '已暂停 · preview-only · 你已请求暂停。\n完成 1 · 待执行 1 · 共 2\n▸ write-notes · 待执行',
  feedback: '', primary: '继续', primaryEnabled: true, stop: '急停', shortcut: 'Ctrl+Alt+F12',
};
export const mocks: Record<string, Snapshot> = {
  paused: common,
  running: { ...common, status: '运行中', tone: 'working', body: '1 / 2 步完成\n写入纪要\n任务执行中；暂停需等待当前操作结束。', primary: '暂停' },
  completed: { ...common, status: '已完成', tone: 'done', body: '2 / 2 步完成 · 1 步有核验记录', primary: '', records: '1. 读取记录 · 已完成 · 有核验记录\n2. 写入纪要 · 已完成', diagnostics: '已完成 · preview-only\n完成 2 · 共 2' },
  review: { ...common, status: '待核对', tone: 'waiting', body: '1 / 2 步完成\n保存纪要\n暂时无法读取上一步结果；继续仅重新核验，不会重做。', primary: '核对结果', diagnostics: '待核对 · preview-only\n▸ save-notes · 待核对 · NOT_FOUND\n最后观察 budget_exhausted · 重试 3 · 剩余 0ms' },
  unknown: { ...common, status: '状态未确认', tone: 'neutral', body: '暂时无法读取任务状态，请稍后查看。', primary: '', records: '步骤记录暂不可读。', diagnostics: 'INTERNAL：状态读取失败' },
  stopped: { ...common, status: '急停已启用', tone: 'error', body: '上一轮执行已停止。\n旧调用不会恢复。', primary: '核对结果', primaryEnabled: false, stop: '修复运行状态', shortcut: '' },
  long: { ...common, title: '整理季度跨部门会议记录与待确认事项，并生成可供复核的完整纪要', body: '2 / 24 步完成\n核对跨部门会议纪要中的待确认事项并保存结果\n观察范围不完整；请使用已确认容器缩小查询范围，不能据此判定目标不存在。\n自动恢复预算已用完。处理现场后点击继续，可重新核查一次；也可停止。', records: Array.from({length: 24}, (_, n) => `${n + 1}. 核对会议记录 ${n + 1} · ${n < 2 ? '已完成' : '待执行'}`).join('\n'), diagnostics: Array.from({length: 18}, (_, n) => `观察 ${n + 1} · NOT_FOUND · 结果未确认，不重放写入`).join('\n') },
};
