# 测试用初始文档

seed.docx 和 seed.xlsx 是本项目生成的最小 OOXML 包，内容为 ACTL seed；没有宏、外链或用户数据。分别供 Word 与 Excel workflow 通过 resources.files.copy 复制到新任务目录。随技能包分发，不能把运行结果写回这些模板。

文件由已移除的 office-package.ps1 在迁移时生成；原始 XML 定义保留在 Git 历史。flow_contract 的 Office 读取测试验证正文、单元格和损坏文件拒绝行为。
