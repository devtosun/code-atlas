import type { Props } from './props';
import { Item as Row } from './Item';

export const GenericList = <T,>({ items, render }: Props<T>) => (
  <section>
    {items.map((item) => <Row value={render(item)} />)}
  </section>
);

export const Empty = () => <Widget />;
