export interface Invoice { amount: number; customer: string; }
export type InvoiceId = string & { readonly invoiceId: unique symbol };
export function total(invoice: Invoice): number { return invoice.amount * 2; }
export function shadow(total: (n: number) => number): number { return total(2); }
// phantom_call() is not a reference or a declaration.
