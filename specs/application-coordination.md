# Application coordination locks

Status: accepted local owner API. `Database::coordination_lock(namespace)` returns
one shared process-local mutex for a live application namespace across every
handle created from that Database owner. At most 64 live locks are retained;
expired weak entries are reclaimed. These handles are not durable identities,
grants, transactions or distributed leases.

An application uses its lock across current authorization checks, durable fresh
claims and the physical local handoff, and uses the same lock for grant mutations.
Independent application adapters must obtain this lock rather than create their
own mutex. Database transactions use their own inner lock and can execute while
the application guard is held. Acquiring the application guard recursively, or
acquiring it inside a database transaction, is unsupported (lock order is
application guard → database transaction).

Native/application storage still enforces its ordinary transaction/CAS rules and
physical owner lock. The coordination API changes no keys, wire bytes, hashes,
readers or persistent layout. Raw trusted database writes are not an application
authorization API. Cross-process ownership remains the backend's exclusive lock.
