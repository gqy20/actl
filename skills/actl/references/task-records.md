# 任务历史与归档

适用于 actl 0.1.6。独立命令可在当前进程环境中设置同一个 `ACTL_TASK_ID`，
值为 1–80 位 ASCII 字母、数字、`-`、`_`。不修改全局环境、不混用其他任务 ID。
流程调用自动使用运行 ID 关联历史。

```powershell
actl history --task report-01 --limit 20
actl history --task report-01 --step check-amount --reports
actl task-archive --file report.json
```

history 的 limit 为 1–100，默认 20。缺少结束事件可能是运行中、中断或日志失败；
complete/integrity 与 logging_health 用于检查记录完整性，不证明业务成功。
没有记录到日志故障，也不保证没有丢失事件。日志有保留期限，证据引用可能已失效。

## 报告格式

report.json 使用 UTF-8，最大 64 KiB：

```json
{
  "task_id": "report-01",
  "outcome": "partial",
  "summary": "已填写金额，尚未确认保存结果",
  "verification": "已读回金额为 1200；保存回执未确认",
  "call_ids": [],
  "feedback": [
    {"source": "agent", "text": "需要核对保存回执", "verified": false}
  ],
  "evidence": []
}
```

替换示例内容，用实际 history 返回的 call_id 填写 call_ids；不编造事件或证据。
outcome 为 completed/partial/stopped/failed，feedback.source 为 agent/user。
summary 和 verification 必须非空；以上字段除 evidence 可省略外均必填。
分开记录执行事实、未验证推测和用户反馈，报告内容由提交者自行精简及脱敏。

归档返回 archived=true 只表示报告已保存，不验证结论。每次新增修订，不覆盖旧报告；
evidence 只存引用，不自动打开或复制截图、文档。输出成果文件应单独保留。
