interface Props {
  title: string;
}

export function TsxCard({ title }: Props) {
  return <article><h1>{title}</h1></article>;
}
