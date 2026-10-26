"""Application definitions imported separately by the typing consumer."""
from sqlaero.retry import RetryRule, retry

@retry(RetryRule(error_types=ValueError), wait_interval=0)
def get_count(user_id: int, *, strict: bool = True) -> int:
    """Return an application integer without losing its callable signature."""
    return user_id if strict else 0
