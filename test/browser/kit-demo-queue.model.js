// The issues in line, running first: (status, props) -> model. Pure.
export function queueModel(status, props) {
  const rows = [...(status?.running ?? []), ...(status?.queued ?? [])].map((i) => ({
    number: i.number,
    title: i.title,
    updatedAt: i.updatedAt,
  }));
  return { rows: rows.slice(0, props.limit ?? rows.length), total: rows.length };
}
