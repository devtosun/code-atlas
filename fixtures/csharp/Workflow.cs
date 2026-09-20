namespace Billing;
public class Workflow
{
    public void Persist(IRepository repository, Invoice invoice)
    {
        repository.Save(invoice); // Interface target is not an exact concrete method.
    }
}
