using System;
using System.Collections.Generic;
using Text = System.String;

namespace Billing.Domain
{
    [AttributeUsage(AttributeTargets.Class)]
    public sealed class AuditAttribute : Attribute
    {
        public AuditAttribute(string category) { }
    }

    public enum InvoiceState { Draft, Sent }

    [Audit("billing")]
    public partial class Ledger<T>
    {
        private readonly IRepository repository;
        public Text Name { get; private set; }

        public Ledger(IRepository repository)
        {
            this.repository = repository;
        }

        public T Echo(T value) => value;
        public T Echo(T value, int count) => value;

        public async System.Threading.Tasks.Task<T> LoadAsync(T value)
        {
            T Normalize(T item) => item;
            await System.Threading.Tasks.Task.Yield();
            return Normalize(value);
        }

        public sealed class Nested
        {
            public void Save(IRepository target, Invoice invoice) => target.Save(invoice);
        }
    }

    public static class InvoiceExtensions
    {
        public static decimal WithTax(this Invoice invoice, decimal rate) => invoice.Total() * rate;
    }
}

#if DEBUG
namespace Billing.Diagnostics { public sealed class DebugProbe { } }
#else
namespace Billing.Diagnostics { public sealed class ReleaseProbe { } }
#endif

