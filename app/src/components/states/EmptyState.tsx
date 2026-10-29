// 空着手的时候说什么。空列表最怕一句「暂无数据」，得说清「现在没有什么」和「接下来去哪儿」。
type Props = {
  /** 一句话说清现在没有什么 */
  title: string;
  /** 可选：补充一句怎么开始 */
  hint?: string;
  /** 可选：给个能点的去处（没有就不画按钮） */
  actionLabel?: string;
  onAction?: () => void;
};

export default function EmptyState({ title, hint, actionLabel, onAction }: Props) {
  return (
    <div className="empty" role="status">
      <div className="empty-title">{title}</div>
      {hint && <p className="note small">{hint}</p>}
      {actionLabel && onAction && (
        <button className="btn small" onClick={onAction}>
          {actionLabel}
        </button>
      )}
    </div>
  );
}
