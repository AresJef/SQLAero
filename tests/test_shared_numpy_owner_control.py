"""A producer's public buffer callbacks end before later source consumption."""

import pytest
from sqlaero import Cursor, DictCursor, SSCursor, SSDictCursor
from sqlaero.errors import InterfaceBusyError, InterfaceError
from test_public_connection import live, requires_mysql

np = pytest.importorskip("numpy")


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("cursor", [Cursor, DictCursor, SSCursor, SSDictCursor])
@pytest.mark.parametrize("prepared", [False, True])
@pytest.mark.parametrize("event", ["acquire", "release", "raise"])
def test_public_buffer_window_preserves_actual_owner_and_original_error(label: str, cursor, prepared: bool, event: str) -> None:
    original = MemoryError("original buffer source error after retirement")
    with live(label) as connection:
        with connection.cursor() as setup:
            setup.execute("CREATE TEMPORARY TABLE owner_control (value BIGINT)")
        context = connection.transaction(cursor, prepared=prepared)
        current = context.__enter__()
        current.execute("INSERT INTO owner_control VALUES (1)")
        exits: list[InterfaceBusyError] = []
        retired = False
        def retire() -> None:
            nonlocal retired
            try:
                context.__exit__(None, None, None)
            except InterfaceBusyError as error:
                exits.append(error)
            retired = True
        class Owner:
            def __init__(self) -> None:
                self.data = np.array([7, 8], dtype="i8")
                self.enabled = False
                self.acquired = 0
                self.released = 0
                self.later = False
            @property
            def base(self) -> None:
                return None
            @property
            def __array_interface__(self) -> dict:
                return self.data.__array_interface__
            def provide(self, _flags: int) -> memoryview:
                if self.enabled:
                    self.acquired += 1
                    self.later |= self.acquired > 2
                    # Export1 is origin preparation; export2 reads actual cell0.
                    if self.acquired == 2 and event in ("acquire", "raise"):
                        retire()
                        if event == "raise":
                            raise original
                return memoryview(self.data)
            def release(self, _view: memoryview) -> None:
                if self.enabled:
                    self.released += 1
                    if self.released == 2 and event == "release":
                        retire()
        owner = Owner()
        source = np.asarray(owner)
        assert source.base is owner
        # Construct through NumPy's public array-interface owner first. PEP688
        # construction otherwise hides it in an opaque CPython buffer wrapper.
        # Public buffer support is then available for the physical interval.
        Owner.__buffer__ = Owner.provide
        Owner.__release_buffer__ = Owner.release
        owner.enabled = True
        try:
            caught: Exception | None = None
            try:
                current.execute("INSERT INTO owner_control VALUES (%s),(%s)", source)
            except Exception as error:
                caught = error
            assert retired and current.closed and not connection.closed and len(exits) == 1, (caught, owner.acquired, owner.released, source.dtype, source.shape)
            assert not owner.later and owner.acquired == 2
            if event == "raise":
                assert caught is original
                assert owner.released == 1
            else:
                assert isinstance(caught, InterfaceError)
                assert owner.released == 2
            with connection.cursor() as check:
                check.execute("SELECT COUNT(*) FROM owner_control")
                assert check.fetchone() == (1,)
            connection.rollback()
            with connection.cursor() as check:
                check.execute("SELECT COUNT(*) FROM owner_control")
                assert check.fetchone() == (0,)
        finally:
            owner.enabled = False
            if not retired:
                context.__exit__(RuntimeError, RuntimeError("test cleanup"), None)
