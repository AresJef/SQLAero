/* Read only CPython's public immutable text metadata, without mirroring layout.
 * The attached Rust caller retains a real str owner throughout the native read.
 */
#include <Python.h>

unsigned int sqlaero_unicode_ascii(PyObject *source)
{
    return PyUnicode_IS_ASCII(source);
}
