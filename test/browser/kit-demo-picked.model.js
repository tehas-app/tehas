// What the picked panel says: (status, picked) -> model. Pure.
export function pickedModel(status, picked) {
  const all = [...(status?.running ?? []), ...(status?.queued ?? [])];
  const issue = all.find((i) => i.number === picked);
  return {
    count: all.length,
    picked: issue ? `#${issue.number} ${issue.title}` : "Nothing picked",
  };
}
