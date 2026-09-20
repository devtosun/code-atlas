import React from 'react';
import { total, type Invoice } from './models';
type Props = { invoice: Invoice; onSelect: (item: Invoice) => void };
export const InvoiceCard = ({ invoice, onSelect }: Props) => (
  <button onClick={() => onSelect(invoice)}>{total(invoice)}</button>
);
