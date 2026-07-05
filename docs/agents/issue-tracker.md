# Issue tracker：GitHub

本仓库的 issue 和 PRD 都存放在 GitHub Issues。所有操作通过 `gh` CLI 完成。

## 约定

- **创建 issue**：`gh issue create --title "..." --body "..."`。多行正文用 heredoc。
- **读取 issue**：`gh issue view <number> --comments`，用 `jq` 过滤评论并一并取标签。
- **列出 issue**：`gh issue list --state open --json number,title,body,labels,comments --jq '[.[] | {number, title, body, labels: [.labels[].name], comments: [.comments[].body]}]'`，按需要加 `--label`、`--state` 过滤。
- **评论**：`gh issue comment <number> --body "..."`
- **加 / 去标签**：`gh issue edit <number> --add-label "..."` / `--remove-label "..."`
- **关闭**：`gh issue close <number> --comment "..."`

仓库归属从 `git remote -v` 推断——`gh` 在 clone 内运行时会自动识别。

## 外部 PR

外部 PR **不**作为请求渠道纳入分诊队列。

## 当技能说"发布到 issue tracker"

创建一条 GitHub issue。

## 当技能说"取回相关 ticket"

运行 `gh issue view <number> --comments`。
