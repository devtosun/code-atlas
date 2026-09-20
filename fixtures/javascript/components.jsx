import { Panel } from './ui.js';

export const Card = ({ value, onSelect }) => (
  <Panel.Item onClick={() => onSelect(value)}>
    <span>{value}</span>
  </Panel.Item>
);

// <Phantom /> and phantom_call() remain comments, not observations.
export const Empty = () => <Widget />;
