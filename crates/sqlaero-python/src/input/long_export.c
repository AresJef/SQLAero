/* Narrow attached-Python ownership boundary for immutable integer digits.
 * CPython >=3.14 provides this API; the pinned upstream compatibility header
 * supplies the same contract on 3.12/3.13. Rust never duplicates PyLong layout.
 */
#include "pythoncapi_compat.h"

typedef struct {
    void *handle;
    const unsigned char *digits;
    size_t count;
    int64_t compact;
    uint8_t bits;
    uint8_t width;
    int8_t order;
    int8_t endian;
    uint8_t negative;
} SqlaeroLongView;

int sqlaero_long_open(PyObject *source, SqlaeroLongView *view)
{
    PyLongExport *exported = PyMem_Malloc(sizeof(*exported));
    if (exported == NULL) {
        PyErr_NoMemory();
        return -1;
    }
    if (PyLong_Export(source, exported) < 0) {
        PyMem_Free(exported);
        return -1;
    }
    const PyLongLayout *layout = PyLong_GetNativeLayout();
    view->handle = exported;
    view->digits = exported->digits;
    /* Only the documented branch's fields are initialized by CPython. */
    if (exported->digits == NULL) {
        view->count = 0;
        view->compact = exported->value;
        view->negative = 0;
    }
    else {
        view->count = (size_t)exported->ndigits;
        view->compact = 0;
        view->negative = exported->negative;
    }
    view->bits = layout->bits_per_digit;
    view->width = layout->digit_size;
    view->order = layout->digits_order;
    view->endian = layout->digit_endianness;
    return 0;
}

void sqlaero_long_close(void *handle)
{
    PyLongExport *exported = handle;
    PyLong_FreeExport(exported);
    PyMem_Free(exported);
}
