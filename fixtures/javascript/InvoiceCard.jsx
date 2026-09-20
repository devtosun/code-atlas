import React from 'react';
export function InvoiceCard({ invoice, onSelect }) {
  return <button onClick={() => onSelect(invoice)}>{invoice.total()}</button>;
}
// JSX must use the JavaScript grammar with the appropriate captures.
