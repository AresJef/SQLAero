# SQLAero error code registry

Generated from data/error-catalog.json (SHA-256 60741095b355992c95153838d9f3f4bbd52d4c57f83e1b4802b1ddb55c9844e2); edit the registry and run tools/error_catalog.py.

Registered official client-message conditions plus applicable shared globals; no log-only MY or other libraries CR codes. Warning/Note responses never automatically raise; mappings apply only to an actual protocol ERR.

A known code has one explicit maintained class. Unknown-code fallback uses only SQLSTATE classes 08/0A/22/23/42; all other, missing, or general states use DatabaseError. Actual ERR numbers and messages are retained.

Only functional names, codes and SQLSTATE facts are extracted from official material; Oracle message templates are not reproduced.

## Client codes

| Code | Member | Exact class | Meaning |
| --- | --- | --- | --- |
| 65536 | NUMERIC_RANGE | DataError | an accurate numeric value exceeds the approved SQL policy. |
| 65537 | SESSION_STATE | InterfaceError | confirmed session state does not admit this operation. |
| 65538 | RESOURCE_LIMIT | OperationalError | a configured native request or result limit was hit. |
| 65539 | PROTOCOL | OperationalError | malformed or incomplete server protocol data. |
| 65540 | UNSUPPORTED_CHARSET | NotSupportedError | a known charset cannot be used by this operation. |
| 65541 | TEMPLATE_BINDING | ProgrammingError | an unsupported active SQL template directive. |
| 65542 | PARAMETER_COUNT | ProgrammingError | current adapter width differs from SQL positions. |
| 65543 | CONTAINER_NESTING | DataError | a true container cycle or excessive active nesting depth. |
| 65544 | OPERATION_DEADLINE | OperationalTimeoutError | the original whole-operation deadline expired. |
| 65545 | DECODE_CONTENT | DataError | a supported result field contains invalid or unrepresentable data. |
| 65546 | INTERFACE_BUSY | InterfaceBusyError | another operation owns this connection or Cursor. |
| 65547 | INTERNAL_STATE | InternalError | a real native ownership/state invariant failed. |
| 65548 | TRANSPORT | OperationalError | actual connection preparation or transport I/O failed. |
| 65549 | TLS | OperationalError | the negotiated TLS channel failed. |
| 65550 | AUTHENTICATION | OperationalError | native authentication could not complete safely. |
| 65551 | UNSUPPORTED_CAPABILITY | NotSupportedError | an actual protocol capability is not implemented. |
| 65552 | CONNECT_DEADLINE | OperationalTimeoutError | original connection establishment deadline. |
| 65553 | READ_DEADLINE | OperationalTimeoutError | a read made no progress before its deadline. |
| 65554 | WRITE_DEADLINE | OperationalTimeoutError | a write made no progress before its deadline. |

## Server codes

| Code | Official symbol(s) | Exact class | Version / SQLSTATE | Meaning |
| --- | --- | --- | --- | --- |
| 1 | EE_CANTCREATEFILE | OperationalError | 8.4: HY000, 9.7: HY000 | A file could not be created; the server message includes the file and operating-system cause |
| 2 | EE_READ | OperationalError | 8.4: HY000, 9.7: HY000 | A file read failed; the server message includes the file and operating-system cause |
| 3 | EE_WRITE | OperationalError | 8.4: HY000, 9.7: HY000 | A file write failed; the server message includes the file and operating-system cause |
| 4 | EE_BADCLOSE | OperationalError | 8.4: HY000, 9.7: HY000 | A file could not be closed; the server message includes the operating-system cause |
| 5 | EE_OUTOFMEMORY | OperationalError | 8.4: HY000, 9.7: HY000 | out of memory |
| 6 | EE_DELETE | OperationalError | 8.4: HY000, 9.7: HY000 | A file could not be deleted; the server message includes the operating-system cause |
| 7 | EE_LINK | OperationalError | 8.4: HY000, 9.7: HY000 | A file could not be renamed; the server message identifies both paths and the operating-system cause |
| 9 | EE_EOFERR | OperationalError | 8.4: HY000, 9.7: HY000 | A file read reached an unexpected end of file |
| 10 | EE_CANTLOCK | OperationalError | 8.4: HY000, 9.7: HY000 | A file lock could not be acquired |
| 11 | EE_CANTUNLOCK | OperationalError | 8.4: HY000, 9.7: HY000 | A file lock could not be released |
| 12 | EE_DIR | OperationalError | 8.4: HY000, 9.7: HY000 | A directory could not be read |
| 13 | EE_STAT | OperationalError | 8.4: HY000, 9.7: HY000 | File metadata could not be obtained from the operating system |
| 14 | EE_CANT_CHSIZE | OperationalError | 8.4: HY000, 9.7: HY000 | A file could not be resized |
| 15 | EE_CANT_OPEN_STREAM | OperationalError | 8.4: HY000, 9.7: HY000 | A file stream could not be opened from its handle |
| 16 | EE_GETWD | OperationalError | 8.4: HY000, 9.7: HY000 | The current working directory could not be obtained |
| 17 | EE_SETWD | OperationalError | 8.4: HY000, 9.7: HY000 | The working directory could not be changed |
| 18 | EE_LINK_WARNING | DatabaseError | 8.4: HY000, 9.7: HY000 | A file has an unexpected number of hard links |
| 19 | EE_OPEN_WARNING | DatabaseError | 8.4: HY000, 9.7: HY000 | Files or streams remain open |
| 20 | EE_DISK_FULL | OperationalError | 8.4: HY000, 9.7: HY000 | disk full |
| 21 | EE_CANT_MKDIR | OperationalError | 8.4: HY000, 9.7: HY000 | A directory could not be created |
| 22 | EE_UNKNOWN_CHARSET | NotSupportedError | 8.4: HY000, 9.7: HY000 | unknown character set |
| 23 | EE_OUT_OF_FILERESOURCES | OperationalError | 8.4: HY000, 9.7: HY000 | The operating system has insufficient resources to open another file |
| 24 | EE_CANT_READLINK | OperationalError | 8.4: HY000, 9.7: HY000 | A symbolic link target could not be read |
| 25 | EE_CANT_SYMLINK | OperationalError | 8.4: HY000, 9.7: HY000 | A symbolic link could not be created |
| 26 | EE_REALPATH | OperationalError | 8.4: HY000, 9.7: HY000 | A path could not be resolved to its canonical filesystem location |
| 27 | EE_SYNC | OperationalError | 8.4: HY000, 9.7: HY000 | A file could not be synchronized to disk |
| 28 | EE_UNKNOWN_COLLATION | NotSupportedError | 8.4: HY000, 9.7: HY000 | unknown collation |
| 29 | EE_FILENOTFOUND | OperationalError | 8.4: HY000, 9.7: HY000 | A required physical file does not exist |
| 30 | EE_FILE_NOT_CLOSED | DatabaseError | 8.4: HY000, 9.7: HY000 | A file remains unclosed |
| 31 | EE_CHANGE_OWNERSHIP | OperationalError | 8.4: HY000, 9.7: HY000 | A file owner could not be changed |
| 32 | EE_CHANGE_PERMISSIONS | OperationalError | 8.4: HY000, 9.7: HY000 | File access permissions could not be changed |
| 33 | EE_CANT_SEEK | OperationalError | 8.4: HY000, 9.7: HY000 | The requested position in a file could not be reached |
| 34 | EE_CAPACITY_EXCEEDED | OperationalError | 8.4: HY000, 9.7: HY000 | A native memory capacity limit was exceeded |
| 35 | EE_DISK_FULL_WITH_RETRY_MSG | OperationalError | 8.4: HY000, 9.7: HY000 | A write is waiting for free disk space after the disk became full |
| 36 | EE_FAILED_TO_CREATE_TIMER | OperationalError | 8.4: HY000, 9.7: HY000 | failed to create timer |
| 37 | EE_FAILED_TO_DELETE_TIMER | OperationalError | 8.4: HY000, 9.7: HY000 | failed to delete timer |
| 38 | EE_FAILED_TO_CREATE_TIMER_QUEUE | OperationalError | 8.4: HY000, 9.7: HY000 | failed to create timer queue |
| 39 | EE_FAILED_TO_START_TIMER_NOTIFY_THREAD | OperationalError | 8.4: HY000, 9.7: HY000 | failed to start timer notify thread |
| 40 | EE_FAILED_TO_CREATE_TIMER_NOTIFY_THREAD_INTERRUPT_EVENT | OperationalError | 8.4: HY000, 9.7: HY000 | A timer notification thread's interrupt event could not be created |
| 41 | EE_EXITING_TIMER_NOTIFY_THREAD | OperationalError | 8.4: HY000, 9.7: HY000 | A timer notification could not be registered while its thread was exiting |
| 42 | EE_WIN_LIBRARY_LOAD_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | A Windows system library could not be loaded |
| 43 | EE_WIN_RUN_TIME_ERROR_CHECK | DatabaseError | 8.4: HY000, 9.7: HY000 | A Windows runtime check reported the problem described by the server message |
| 44 | EE_FAILED_TO_DETERMINE_LARGE_PAGE_SIZE | OperationalError | 8.4: HY000, 9.7: HY000 | failed to determine large page size |
| 45 | EE_FAILED_TO_KILL_ALL_THREADS | OperationalError | 8.4: HY000, 9.7: HY000 | failed to kill all threads |
| 46 | EE_FAILED_TO_CREATE_IO_COMPLETION_PORT | OperationalError | 8.4: HY000, 9.7: HY000 | An operating-system I/O completion port could not be created |
| 47 | EE_FAILED_TO_OPEN_DEFAULTS_FILE | OperationalError | 8.4: HY000, 9.7: HY000 | failed to open defaults file |
| 48 | EE_FAILED_TO_HANDLE_DEFAULTS_FILE | OperationalError | 8.4: HY000, 9.7: HY000 | failed to handle defaults file |
| 49 | EE_WRONG_DIRECTIVE_IN_CONFIG_FILE | ProgrammingError | 8.4: HY000, 9.7: HY000 | invalid directive in config file |
| 50 | EE_SKIPPING_DIRECTIVE_DUE_TO_MAX_INCLUDE_RECURSION | OperationalError | 8.4: HY000, 9.7: HY000 | A configuration include was skipped because the include recursion limit was reached |
| 51 | EE_INCORRECT_GRP_DEFINITION_IN_CONFIG_FILE | ProgrammingError | 8.4: HY000, 9.7: HY000 | A configuration group declaration is invalid |
| 52 | EE_OPTION_WITHOUT_GRP_IN_CONFIG_FILE | ProgrammingError | 8.4: HY000, 9.7: HY000 | A configuration option appears before any group declaration |
| 53 | EE_CONFIG_FILE_PERMISSION_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | config file permission error |
| 54 | EE_IGNORE_WORLD_WRITABLE_CONFIG_FILE | OperationalError | 8.4: HY000, 9.7: HY000 | ignore world writable config file |
| 55 | EE_USING_DISABLED_OPTION | NotSupportedError | 8.4: HY000, 9.7: HY000 | using disabled option |
| 56 | EE_USING_DISABLED_SHORT_OPTION | NotSupportedError | 8.4: HY000, 9.7: HY000 | using disabled short option |
| 57 | EE_USING_PASSWORD_ON_CLI_IS_INSECURE | DatabaseError | 8.4: HY000, 9.7: HY000 | Supplying a password on the command line can expose the password |
| 58 | EE_UNKNOWN_SUFFIX_FOR_VARIABLE | DataError | 8.4: HY000, 9.7: HY000 | unknown suffix for variable |
| 59 | EE_SSL_ERROR_FROM_FILE | OperationalError | 8.4: HY000, 9.7: HY000 | SSL error from file |
| 60 | EE_SSL_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | SSL error |
| 61 | EE_NET_SEND_ERROR_IN_BOOTSTRAP | OperationalError | 8.4: HY000, 9.7: HY000 | Sending a network packet during server bootstrap failed |
| 62 | EE_PACKETS_OUT_OF_ORDER | OperationalError | 8.4: HY000, 9.7: HY000 | packets out of order |
| 63 | EE_UNKNOWN_PROTOCOL_OPTION | ProgrammingError | 8.4: HY000, 9.7: HY000 | unknown protocol option |
| 64 | EE_FAILED_TO_LOCATE_SERVER_PUBLIC_KEY | OperationalError | 8.4: HY000, 9.7: HY000 | failed to locate server public key |
| 65 | EE_PUBLIC_KEY_NOT_IN_PEM_FORMAT | OperationalError | 8.4: HY000, 9.7: HY000 | public key not in pem format |
| 66 | EE_DEBUG_INFO | DatabaseError | 8.4: HY000, 9.7: HY000 | The server reports the debug diagnostic contained in its message |
| 67 | EE_UNKNOWN_VARIABLE | ProgrammingError | 8.4: HY000, 9.7: HY000 | unknown variable |
| 68 | EE_UNKNOWN_OPTION | ProgrammingError | 8.4: HY000, 9.7: HY000 | unknown option |
| 69 | EE_UNKNOWN_SHORT_OPTION | ProgrammingError | 8.4: HY000, 9.7: HY000 | unknown short option |
| 70 | EE_OPTION_WITHOUT_ARGUMENT | ProgrammingError | 8.4: HY000, 9.7: HY000 | An option that accepts no argument was given one |
| 71 | EE_OPTION_REQUIRES_ARGUMENT | ProgrammingError | 8.4: HY000, 9.7: HY000 | option requires argument |
| 72 | EE_SHORT_OPTION_REQUIRES_ARGUMENT | ProgrammingError | 8.4: HY000, 9.7: HY000 | short option requires argument |
| 73 | EE_OPTION_IGNORED_DUE_TO_INVALID_VALUE | DataError | 8.4: HY000, 9.7: HY000 | option ignored due to invalid value |
| 74 | EE_OPTION_WITH_EMPTY_VALUE | DataError | 8.4: HY000, 9.7: HY000 | option with empty value |
| 75 | EE_FAILED_TO_ASSIGN_MAX_VALUE_TO_OPTION | ProgrammingError | 8.4: HY000, 9.7: HY000 | The option does not support setting a separate maximum value |
| 76 | EE_INCORRECT_BOOLEAN_VALUE_FOR_OPTION | DataError | 8.4: HY000, 9.7: HY000 | An invalid boolean option value was replaced with OFF |
| 77 | EE_FAILED_TO_SET_OPTION_VALUE | DatabaseError | 8.4: HY000, 9.7: HY000 | failed to set option value |
| 78 | EE_INCORRECT_INT_VALUE_FOR_OPTION | DataError | 8.4: HY000, 9.7: HY000 | incorrect int value for option |
| 79 | EE_INCORRECT_UINT_VALUE_FOR_OPTION | DataError | 8.4: HY000, 9.7: HY000 | incorrect uint value for option |
| 80 | EE_ADJUSTED_SIGNED_VALUE_FOR_OPTION | DataError | 8.4: HY000, 9.7: HY000 | adjusted signed value for option |
| 81 | EE_ADJUSTED_UNSIGNED_VALUE_FOR_OPTION | DataError | 8.4: HY000, 9.7: HY000 | adjusted unsigned value for option |
| 82 | EE_ADJUSTED_ULONGLONG_VALUE_FOR_OPTION | DataError | 8.4: HY000, 9.7: HY000 | An unsigned 64-bit option value was adjusted to the permitted range |
| 83 | EE_ADJUSTED_DOUBLE_VALUE_FOR_OPTION | DataError | 8.4: HY000, 9.7: HY000 | adjusted double value for option |
| 84 | EE_INVALID_DECIMAL_VALUE_FOR_OPTION | DataError | 8.4: HY000, 9.7: HY000 | invalid decimal value for option |
| 85 | EE_COLLATION_PARSER_ERROR | DataError | 8.4: HY000, 9.7: HY000 | collation parser error |
| 86 | EE_FAILED_TO_RESET_BEFORE_PRIMARY_IGNORABLE_CHAR | DataError | 8.4: HY000, 9.7: HY000 | A collation cannot reset before a primary-ignorable character |
| 87 | EE_FAILED_TO_RESET_BEFORE_TERTIARY_IGNORABLE_CHAR | DataError | 8.4: HY000, 9.7: HY000 | A collation cannot reset before a tertiary-ignorable character |
| 88 | EE_SHIFT_CHAR_OUT_OF_RANGE | DataError | 8.4: HY000, 9.7: HY000 | The collation's shifted character is outside the supported range |
| 89 | EE_RESET_CHAR_OUT_OF_RANGE | DataError | 8.4: HY000, 9.7: HY000 | The collation's reset character is outside the supported range |
| 90 | EE_UNKNOWN_LDML_TAG | DataError | 8.4: HY000, 9.7: HY000 | The collation definition contains an unknown Locale Data Markup Language tag |
| 91 | EE_FAILED_TO_RESET_BEFORE_SECONDARY_IGNORABLE_CHAR | DataError | 8.4: HY000, 9.7: HY000 | A collation cannot reset before a secondary-ignorable character |
| 92 | EE_FAILED_PROCESSING_DIRECTIVE | DatabaseError | 8.4: HY000, 9.7: HY000 | Processing a configuration directive failed |
| 93 | EE_PTHREAD_KILL_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | Sending a signal to an operating-system thread failed |
| 94 | EE_COLLATION_ALIAS_ERROR | ProgrammingError | 9.7: HY000 | a collation name must be specified explicitly |
| 1004 | ER_CANT_CREATE_FILE | OperationalError | 8.4: HY000, 9.7: HY000 | cannot create file |
| 1005 | ER_CANT_CREATE_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot create table |
| 1006 | ER_CANT_CREATE_DB | OperationalError | 8.4: HY000, 9.7: HY000 | cannot create database |
| 1007 | ER_DB_CREATE_EXISTS | ProgrammingError | 8.4: HY000, 9.7: HY000 | The database to be created already exists |
| 1008 | ER_DB_DROP_EXISTS | ProgrammingError | 8.4: HY000, 9.7: HY000 | The database to be dropped does not exist |
| 1010 | ER_DB_DROP_RMDIR | OperationalError | 8.4: HY000, 9.7: HY000 | Dropping a database failed while removing its directory |
| 1012 | ER_CANT_FIND_SYSTEM_REC | OperationalError | 8.4: HY000, 9.7: HY000 | A required record is missing from a system table |
| 1013 | ER_CANT_GET_STAT | OperationalError | 8.4: HY000, 9.7: HY000 | The operating system could not return a file's metadata |
| 1015 | ER_CANT_LOCK | OperationalError | 8.4: HY000, 9.7: HY000 | A file or table lock could not be acquired |
| 1016 | ER_CANT_OPEN_FILE | OperationalError | 8.4: HY000, 9.7: HY000 | cannot open file |
| 1017 | ER_FILE_NOT_FOUND | OperationalError | 8.4: HY000, 9.7: HY000 | file not found |
| 1018 | ER_CANT_READ_DIR | OperationalError | 8.4: HY000, 9.7: HY000 | The database directory could not be read |
| 1020 | ER_CHECKREAD | OperationalError | 8.4: HY000, 9.7: HY000 | A record changed after it was read for update |
| 1022 | ER_DUP_KEY | IntegrityError | 8.4: 23000, 9.7: 23000 | Writing a row would duplicate a unique key |
| 1024 | ER_ERROR_ON_READ | OperationalError | 8.4: HY000, 9.7: HY000 | A storage file read failed |
| 1025 | ER_ERROR_ON_RENAME | OperationalError | 8.4: HY000, 9.7: HY000 | A storage file rename failed |
| 1026 | ER_ERROR_ON_WRITE | OperationalError | 8.4: HY000, 9.7: HY000 | A storage file write failed |
| 1027 | ER_FILE_USED | OperationalError | 8.4: HY000, 9.7: HY000 | A file is locked against change by another user |
| 1030 | ER_GET_ERRNO | DatabaseError | 8.4: HY000, 9.7: HY000 | The storage engine reported the error identified in the server message |
| 1031 | ER_ILLEGAL_HA | NotSupportedError | 8.4: HY000, 9.7: HY000 | The table's storage engine does not support the requested operation |
| 1032 | ER_KEY_NOT_FOUND | DatabaseError | 8.4: HY000, 9.7: HY000 | The requested record was not found by its key |
| 1033 | ER_NOT_FORM_FILE | InternalError | 8.4: HY000, 9.7: HY000 | The table definition file has an invalid format |
| 1034 | ER_NOT_KEYFILE | InternalError | 8.4: HY000, 9.7: HY000 | A table index file is corrupt |
| 1035 | ER_OLD_KEYFILE | OperationalError | 8.4: HY000, 9.7: HY000 | A table index file uses an obsolete format |
| 1036 | ER_OPEN_AS_READONLY | OperationalError | 8.4: HY000, 9.7: HY000 | The table is read-only |
| 1037 | ER_OUTOFMEMORY | OperationalError | 8.4: HY001, 9.7: HY001 | out of memory |
| 1038 | ER_OUT_OF_SORTMEMORY | OperationalError | 8.4: HY001, 9.7: HY001 | The server has insufficient memory for sorting |
| 1040 | ER_CON_COUNT_ERROR | OperationalError | 8.4: 08004, 9.7: 08004 | The server connection limit has been reached |
| 1041 | ER_OUT_OF_RESOURCES | OperationalError | 8.4: HY000, 9.7: HY000 | out of resources |
| 1042 | ER_BAD_HOST_ERROR | OperationalError | 8.4: 08S01, 9.7: 08S01 | The connecting host name or address could not be resolved |
| 1043 | ER_HANDSHAKE_ERROR | OperationalError | 8.4: 08S01, 9.7: 08S01 | handshake error |
| 1044 | ER_DBACCESS_DENIED_ERROR | OperationalError | 8.4: 42000, 9.7: 42000 | The account is not permitted to access the database |
| 1045 | ER_ACCESS_DENIED_ERROR | OperationalError | 8.4: 28000, 9.7: 28000 | access denied error |
| 1046 | ER_NO_DB_ERROR | ProgrammingError | 8.4: 3D000, 9.7: 3D000 | No default database has been selected |
| 1047 | ER_UNKNOWN_COM_ERROR | NotSupportedError | 8.4: 08S01, 9.7: 08S01 | The server received an unknown command |
| 1048 | ER_BAD_NULL_ERROR | IntegrityError | 8.4: 23000, 9.7: 23000 | A NULL value was supplied for a column that does not allow NULL |
| 1049 | ER_BAD_DB_ERROR | ProgrammingError | 8.4: 42000, 9.7: 42000 | The requested database does not exist |
| 1050 | ER_TABLE_EXISTS_ERROR | ProgrammingError | 8.4: 42S01, 9.7: 42S01 | table exists error |
| 1051 | ER_BAD_TABLE_ERROR | ProgrammingError | 8.4: 42S02, 9.7: 42S02 | The requested table does not exist |
| 1052 | ER_NON_UNIQ_ERROR | ProgrammingError | 8.4: 23000, 9.7: 23000 | ambiguous SQL column reference requiring qualification |
| 1053 | ER_SERVER_SHUTDOWN | OperationalError | 8.4: 08S01, 9.7: 08S01 | server shutdown |
| 1054 | ER_BAD_FIELD_ERROR | ProgrammingError | 8.4: 42S22, 9.7: 42S22 | A referenced column cannot be found |
| 1055 | ER_WRONG_FIELD_WITH_GROUP | ProgrammingError | 8.4: 42000, 9.7: 42000 | A selected expression violates the query's GROUP BY rules |
| 1056 | ER_WRONG_GROUP_FIELD | ProgrammingError | 8.4: 42000, 9.7: 42000 | A column is not valid in the GROUP BY clause |
| 1057 | ER_WRONG_SUM_SELECT | ProgrammingError | 8.4: 42000, 9.7: 42000 | The query mixes incompatible aggregate and selected expressions |
| 1058 | ER_WRONG_VALUE_COUNT | ProgrammingError | 8.4: 21S01, 9.7: 21S01 | The number of supplied values is invalid |
| 1059 | ER_TOO_LONG_IDENT | ProgrammingError | 8.4: 42000, 9.7: 42000 | An identifier exceeds the permitted length |
| 1060 | ER_DUP_FIELDNAME | ProgrammingError | 8.4: 42S21, 9.7: 42S21 | A column name is specified more than once |
| 1061 | ER_DUP_KEYNAME | ProgrammingError | 8.4: 42000, 9.7: 42000 | An index name is specified more than once |
| 1062 | ER_DUP_ENTRY | IntegrityError | 8.4: 23000, 9.7: 23000 | A row would duplicate an existing primary or unique key |
| 1063 | ER_WRONG_FIELD_SPEC | ProgrammingError | 8.4: 42000, 9.7: 42000 | A column definition contains invalid attributes |
| 1064 | ER_PARSE_ERROR | ProgrammingError | 8.4: 42000, 9.7: 42000 | parse error |
| 1065 | ER_EMPTY_QUERY | ProgrammingError | 8.4: 42000, 9.7: 42000 | empty query |
| 1066 | ER_NONUNIQ_TABLE | ProgrammingError | 8.4: 42000, 9.7: 42000 | A table name or alias is used more than once in the query |
| 1067 | ER_INVALID_DEFAULT | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid default |
| 1068 | ER_MULTIPLE_PRI_KEY | ProgrammingError | 8.4: 42000, 9.7: 42000 | A table declares more than one primary key |
| 1069 | ER_TOO_MANY_KEYS | ProgrammingError | 8.4: 42000, 9.7: 42000 | too many keys |
| 1070 | ER_TOO_MANY_KEY_PARTS | ProgrammingError | 8.4: 42000, 9.7: 42000 | An index has too many component columns |
| 1071 | ER_TOO_LONG_KEY | ProgrammingError | 8.4: 42000, 9.7: 42000 | An index key exceeds the storage engine's maximum length |
| 1072 | ER_KEY_COLUMN_DOES_NOT_EXITS | ProgrammingError | 8.4: 42000, 9.7: 42000 | An indexed column does not exist in the table |
| 1073 | ER_BLOB_USED_AS_KEY | NotSupportedError | 8.4: 42000, 9.7: 42000 | The column's type cannot be used as the requested key |
| 1074 | ER_TOO_BIG_FIELDLENGTH | ProgrammingError | 8.4: 42000, 9.7: 42000 | A column's declared length exceeds the permitted limit |
| 1075 | ER_WRONG_AUTO_KEY | ProgrammingError | 8.4: 42000, 9.7: 42000 | An AUTO_INCREMENT column or its supporting key is defined incorrectly |
| 1076 | ER_READY | DatabaseError | 8.4: HY000, 9.7: HY000 | The server is ready to accept connections |
| 1079 | ER_SHUTDOWN_COMPLETE | DatabaseError | 8.4: HY000, 9.7: HY000 | shutdown complete |
| 1080 | ER_FORCING_CLOSE | OperationalError | 8.4: 08S01, 9.7: 08S01 | forcing close |
| 1081 | ER_IPSOCK_ERROR | OperationalError | 8.4: 08S01, 9.7: 08S01 | An IP socket could not be created |
| 1082 | ER_NO_SUCH_INDEX | ProgrammingError | 8.4: 42S12, 9.7: 42S12 | no such index |
| 1083 | ER_WRONG_FIELD_TERMINATORS | ProgrammingError | 8.4: 42000, 9.7: 42000 | The field or line separator specification is invalid |
| 1084 | ER_BLOBS_AND_NO_TERMINATED | ProgrammingError | 8.4: 42000, 9.7: 42000 | A variable-length column requires field termination in this text-file operation |
| 1085 | ER_TEXTFILE_NOT_READABLE | OperationalError | 8.4: HY000, 9.7: HY000 | textfile not readable |
| 1086 | ER_FILE_EXISTS_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | file exists error |
| 1087 | ER_LOAD_INFO | DatabaseError | 8.4: HY000, 9.7: HY000 | LOAD DATA reports the record and warning counts in its result message |
| 1088 | ER_ALTER_INFO | DatabaseError | 8.4: HY000, 9.7: HY000 | ALTER TABLE reports the record and duplicate counts in its result message |
| 1089 | ER_WRONG_SUB_KEY | ProgrammingError | 8.4: HY000, 9.7: HY000 | An index prefix is invalid for the column or storage engine |
| 1090 | ER_CANT_REMOVE_ALL_FIELDS | ProgrammingError | 8.4: 42000, 9.7: 42000 | cannot remove all fields |
| 1091 | ER_CANT_DROP_FIELD_OR_KEY | ProgrammingError | 8.4: 42000, 9.7: 42000 | cannot drop field or key |
| 1092 | ER_INSERT_INFO | DatabaseError | 8.4: HY000, 9.7: HY000 | INSERT reports the record, duplicate and warning counts in its result message |
| 1093 | ER_UPDATE_TABLE_USED | ProgrammingError | 8.4: HY000, 9.7: HY000 | The statement reads the table it is also modifying in a prohibited context |
| 1094 | ER_NO_SUCH_THREAD | ProgrammingError | 8.4: HY000, 9.7: HY000 | no such thread |
| 1095 | ER_KILL_DENIED_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | The account is not permitted to terminate the specified thread |
| 1096 | ER_NO_TABLES_USED | ProgrammingError | 8.4: HY000, 9.7: HY000 | no tables used |
| 1097 | ER_TOO_BIG_SET | ProgrammingError | 8.4: HY000, 9.7: HY000 | A SET column declares too many members |
| 1098 | ER_NO_UNIQUE_LOGFILE | OperationalError | 8.4: HY000, 9.7: HY000 | A unique log file name could not be generated |
| 1099 | ER_TABLE_NOT_LOCKED_FOR_WRITE | ProgrammingError | 8.4: HY000, 9.7: HY000 | table not locked for write |
| 1100 | ER_TABLE_NOT_LOCKED | ProgrammingError | 8.4: HY000, 9.7: HY000 | The requested table has not been locked by this session |
| 1101 | ER_BLOB_CANT_HAVE_DEFAULT | ProgrammingError | 8.4: 42000, 9.7: 42000 | blob cannot have default |
| 1102 | ER_WRONG_DB_NAME | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid database name |
| 1103 | ER_WRONG_TABLE_NAME | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid table name |
| 1104 | ER_TOO_BIG_SELECT | OperationalError | 8.4: 42000, 9.7: 42000 | The query would examine more rows than the configured limit permits |
| 1105 | ER_UNKNOWN_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | unknown error |
| 1106 | ER_UNKNOWN_PROCEDURE | ProgrammingError | 8.4: 42000, 9.7: 42000 | unknown procedure |
| 1107 | ER_WRONG_PARAMCOUNT_TO_PROCEDURE | ProgrammingError | 8.4: 42000, 9.7: 42000 | A stored procedure was called with an invalid argument count |
| 1108 | ER_WRONG_PARAMETERS_TO_PROCEDURE | ProgrammingError | 8.4: HY000, 9.7: HY000 | A stored procedure was called with invalid arguments |
| 1109 | ER_UNKNOWN_TABLE | ProgrammingError | 8.4: 42S02, 9.7: 42S02 | unknown table |
| 1110 | ER_FIELD_SPECIFIED_TWICE | ProgrammingError | 8.4: 42000, 9.7: 42000 | field specified twice |
| 1111 | ER_INVALID_GROUP_FUNC_USE | ProgrammingError | 8.4: HY000, 9.7: HY000 | An aggregate function is used in an invalid context |
| 1112 | ER_UNSUPPORTED_EXTENSION | NotSupportedError | 8.4: 42000, 9.7: 42000 | unsupported extension |
| 1113 | ER_TABLE_MUST_HAVE_COLUMNS | ProgrammingError | 8.4: 42000, 9.7: 42000 | table must have columns |
| 1114 | ER_RECORD_FILE_FULL | OperationalError | 8.4: HY000, 9.7: HY000 | The table's data file or storage capacity is full |
| 1115 | ER_UNKNOWN_CHARACTER_SET | ProgrammingError | 8.4: 42000, 9.7: 42000 | unknown character set |
| 1116 | ER_TOO_MANY_TABLES | ProgrammingError | 8.4: HY000, 9.7: HY000 | too many tables |
| 1117 | ER_TOO_MANY_FIELDS | ProgrammingError | 8.4: HY000, 9.7: HY000 | too many fields |
| 1118 | ER_TOO_BIG_ROWSIZE | ProgrammingError | 8.4: 42000, 9.7: 42000 | A row exceeds the storage engine's maximum row size |
| 1119 | ER_STACK_OVERRUN | OperationalError | 8.4: HY000, 9.7: HY000 | stack overrun |
| 1120 | ER_WRONG_OUTER_JOIN_UNUSED | ProgrammingError | 8.4: 42000, 9.7: 42000 | The outer join has an invalid table dependency |
| 1121 | ER_NULL_COLUMN_IN_INDEX | NotSupportedError | 8.4: 42000, 9.7: 42000 | A column used by this index permits NULL when the index requires non-NULL values |
| 1122 | ER_CANT_FIND_UDF | OperationalError | 8.4: HY000, 9.7: HY000 | The named loadable function could not be found |
| 1123 | ER_CANT_INITIALIZE_UDF | OperationalError | 8.4: HY000, 9.7: HY000 | The named loadable function could not be initialized |
| 1124 | ER_UDF_NO_PATHS | ProgrammingError | 8.4: HY000, 9.7: HY000 | A loadable function's library name must not contain a path |
| 1125 | ER_UDF_EXISTS | ProgrammingError | 8.4: HY000, 9.7: HY000 | The loadable function already exists |
| 1126 | ER_CANT_OPEN_LIBRARY | OperationalError | 8.4: HY000, 9.7: HY000 | cannot open library |
| 1127 | ER_CANT_FIND_DL_ENTRY | OperationalError | 8.4: HY000, 9.7: HY000 | The required entry point could not be found in a shared library |
| 1128 | ER_FUNCTION_NOT_DEFINED | ProgrammingError | 8.4: HY000, 9.7: HY000 | function not defined |
| 1129 | ER_HOST_IS_BLOCKED | OperationalError | 8.4: HY000, 9.7: HY000 | host is blocked |
| 1130 | ER_HOST_NOT_PRIVILEGED | OperationalError | 8.4: HY000, 9.7: HY000 | The connecting host is not permitted by the account's host rules |
| 1131 | ER_PASSWORD_ANONYMOUS_USER | ProgrammingError | 8.4: 42000, 9.7: 42000 | An anonymous account cannot change its password in this way |
| 1132 | ER_PASSWORD_NOT_ALLOWED | OperationalError | 8.4: 42000, 9.7: 42000 | A password is not permitted for this account or operation |
| 1133 | ER_PASSWORD_NO_MATCH | ProgrammingError | 8.4: 42000, 9.7: 42000 | No matching account exists for the password change |
| 1134 | ER_UPDATE_INFO | DatabaseError | 8.4: HY000, 9.7: HY000 | UPDATE reports the matched, changed and warning counts in its result message |
| 1135 | ER_CANT_CREATE_THREAD | OperationalError | 8.4: HY000, 9.7: HY000 | cannot create thread |
| 1136 | ER_WRONG_VALUE_COUNT_ON_ROW | ProgrammingError | 8.4: 21S01, 9.7: 21S01 | A row's value count does not match the target column count |
| 1137 | ER_CANT_REOPEN_TABLE | OperationalError | 8.4: HY000, 9.7: HY000 | cannot reopen table |
| 1138 | ER_INVALID_USE_OF_NULL | DataError | 8.4: 22004, 9.7: 22004 | invalid use of null |
| 1139 | ER_REGEXP_ERROR | DatabaseError | 8.4: 42000, 9.7: 42000 | A regular expression is invalid |
| 1140 | ER_MIX_OF_GROUP_FUNC_AND_FIELDS | ProgrammingError | 8.4: 42000, 9.7: 42000 | Nonaggregated expressions are mixed with aggregate expressions without a valid grouping |
| 1141 | ER_NONEXISTING_GRANT | ProgrammingError | 8.4: 42000, 9.7: 42000 | The requested account grant does not exist |
| 1142 | ER_TABLEACCESS_DENIED_ERROR | OperationalError | 8.4: 42000, 9.7: 42000 | The account lacks the required table privilege |
| 1143 | ER_COLUMNACCESS_DENIED_ERROR | OperationalError | 8.4: 42000, 9.7: 42000 | The account lacks the required column privilege |
| 1144 | ER_ILLEGAL_GRANT_FOR_TABLE | ProgrammingError | 8.4: 42000, 9.7: 42000 | illegal grant for table |
| 1145 | ER_GRANT_WRONG_HOST_OR_USER | ProgrammingError | 8.4: 42000, 9.7: 42000 | A GRANT statement specifies an invalid host or user |
| 1146 | ER_NO_SUCH_TABLE | ProgrammingError | 8.4: 42S02, 9.7: 42S02 | no such table |
| 1147 | ER_NONEXISTING_TABLE_GRANT | ProgrammingError | 8.4: 42000, 9.7: 42000 | The requested table-level grant does not exist |
| 1148 | ER_NOT_ALLOWED_COMMAND | NotSupportedError | 8.4: 42000, 9.7: 42000 | not allowed command |
| 1149 | ER_SYNTAX_ERROR | ProgrammingError | 8.4: 42000, 9.7: 42000 | syntax error |
| 1152 | ER_ABORTING_CONNECTION | OperationalError | 8.4: 08S01, 9.7: 08S01 | aborting connection |
| 1153 | ER_NET_PACKET_TOO_LARGE | OperationalError | 8.4: 08S01, 9.7: 08S01 | net packet too large |
| 1154 | ER_NET_READ_ERROR_FROM_PIPE | OperationalError | 8.4: 08S01, 9.7: 08S01 | A network read from a pipe failed |
| 1155 | ER_NET_FCNTL_ERROR | OperationalError | 8.4: 08S01, 9.7: 08S01 | The server could not configure a network file descriptor |
| 1156 | ER_NET_PACKETS_OUT_OF_ORDER | OperationalError | 8.4: 08S01, 9.7: 08S01 | net packets out of order |
| 1157 | ER_NET_UNCOMPRESS_ERROR | OperationalError | 8.4: 08S01, 9.7: 08S01 | A compressed network packet could not be decompressed |
| 1158 | ER_NET_READ_ERROR | OperationalError | 8.4: 08S01, 9.7: 08S01 | net read error |
| 1159 | ER_NET_READ_INTERRUPTED | OperationalError | 8.4: 08S01, 9.7: 08S01 | A network read timed out |
| 1160 | ER_NET_ERROR_ON_WRITE | OperationalError | 8.4: 08S01, 9.7: 08S01 | net error on write |
| 1161 | ER_NET_WRITE_INTERRUPTED | OperationalError | 8.4: 08S01, 9.7: 08S01 | A network write timed out |
| 1162 | ER_TOO_LONG_STRING | DataError | 8.4: 42000, 9.7: 42000 | A supplied string exceeds the permitted length |
| 1163 | ER_TABLE_CANT_HANDLE_BLOB | NotSupportedError | 8.4: 42000, 9.7: 42000 | The table's storage engine does not support BLOB or TEXT columns |
| 1164 | ER_TABLE_CANT_HANDLE_AUTO_INCREMENT | NotSupportedError | 8.4: 42000, 9.7: 42000 | The table's storage engine does not support AUTO_INCREMENT |
| 1166 | ER_WRONG_COLUMN_NAME | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid column name |
| 1167 | ER_WRONG_KEY_COLUMN | NotSupportedError | 8.4: 42000, 9.7: 42000 | invalid key column |
| 1168 | ER_WRONG_MRG_TABLE | ProgrammingError | 8.4: HY000, 9.7: HY000 | The tables in a MERGE table have incompatible definitions |
| 1169 | ER_DUP_UNIQUE | IntegrityError | 8.4: 23000, 9.7: 23000 | A row would duplicate a unique constraint |
| 1170 | ER_BLOB_KEY_WITHOUT_LENGTH | ProgrammingError | 8.4: 42000, 9.7: 42000 | A BLOB or TEXT index requires an explicit prefix length |
| 1171 | ER_PRIMARY_CANT_HAVE_NULL | ProgrammingError | 8.4: 42000, 9.7: 42000 | A primary key column must not permit NULL |
| 1172 | ER_TOO_MANY_ROWS | ProgrammingError | 8.4: 42000, 9.7: 42000 | too many rows |
| 1173 | ER_REQUIRES_PRIMARY_KEY | ProgrammingError | 8.4: 42000, 9.7: 42000 | requires primary key |
| 1175 | ER_UPDATE_WITHOUT_KEY_IN_SAFE_MODE | ProgrammingError | 8.4: HY000, 9.7: HY000 | update without key in safe mode |
| 1176 | ER_KEY_DOES_NOT_EXITS | ProgrammingError | 8.4: 42000, 9.7: 42000 | The named index does not exist in the table |
| 1177 | ER_CHECK_NO_SUCH_TABLE | DatabaseError | 8.4: 42000, 9.7: 42000 | The table to be checked does not exist |
| 1178 | ER_CHECK_NOT_IMPLEMENTED | NotSupportedError | 8.4: 42000, 9.7: 42000 | The storage engine does not implement this table check |
| 1179 | ER_CANT_DO_THIS_DURING_AN_TRANSACTION | ProgrammingError | 8.4: 25000, 9.7: 25000 | The operation is not allowed inside the current transaction |
| 1180 | ER_ERROR_DURING_COMMIT | OperationalError | 8.4: HY000, 9.7: HY000 | error during commit |
| 1181 | ER_ERROR_DURING_ROLLBACK | OperationalError | 8.4: HY000, 9.7: HY000 | error during rollback |
| 1182 | ER_ERROR_DURING_FLUSH_LOGS | OperationalError | 8.4: HY000, 9.7: HY000 | error during flush logs |
| 1184 | ER_NEW_ABORTING_CONNECTION | OperationalError | 8.4: 08S01, 9.7: 08S01 | A connection was aborted for the reason in the server message |
| 1188 | ER_SOURCE | OperationalError | 8.4: HY000, 9.7: HY000 | The replication source reported an error |
| 1189 | ER_SOURCE_NET_READ | OperationalError | 8.4: 08S01, 9.7: 08S01 | Reading from the replication source failed |
| 1190 | ER_SOURCE_NET_WRITE | OperationalError | 8.4: 08S01, 9.7: 08S01 | Writing to the replication source failed |
| 1191 | ER_FT_MATCHING_KEY_NOT_FOUND | ProgrammingError | 8.4: HY000, 9.7: HY000 | No FULLTEXT index matches the requested column list |
| 1192 | ER_LOCK_OR_ACTIVE_TRANSACTION | ProgrammingError | 8.4: HY000, 9.7: HY000 | The operation cannot run while the session has locked tables or an active transaction |
| 1193 | ER_UNKNOWN_SYSTEM_VARIABLE | ProgrammingError | 8.4: HY000, 9.7: HY000 | unknown system variable |
| 1194 | ER_CRASHED_ON_USAGE | InternalError | 8.4: HY000, 9.7: HY000 | The table is marked as crashed and requires repair |
| 1195 | ER_CRASHED_ON_REPAIR | InternalError | 8.4: HY000, 9.7: HY000 | Repairing the crashed table failed |
| 1196 | ER_WARNING_NOT_COMPLETE_ROLLBACK | OperationalError | 8.4: HY000, 9.7: HY000 | Rollback could not undo changes to nontransactional tables |
| 1197 | ER_TRANS_CACHE_FULL | OperationalError | 8.4: HY000, 9.7: HY000 | The transaction's binary log cache is full |
| 1199 | ER_REPLICA_NOT_RUNNING | OperationalError | 8.4: HY000, 9.7: HY000 | replica not running |
| 1200 | ER_BAD_REPLICA | OperationalError | 8.4: HY000, 9.7: HY000 | Replication is not configured for this replica |
| 1201 | ER_CONNECTION_METADATA | OperationalError | 8.4: HY000, 9.7: HY000 | Replication connection metadata could not be initialized |
| 1202 | ER_REPLICA_THREAD | OperationalError | 8.4: HY000, 9.7: HY000 | A replica thread could not be created |
| 1203 | ER_TOO_MANY_USER_CONNECTIONS | OperationalError | 8.4: 42000, 9.7: 42000 | too many user connections |
| 1204 | ER_SET_CONSTANTS_ONLY | ProgrammingError | 8.4: HY000, 9.7: HY000 | SET requires constant values in this context |
| 1205 | ER_LOCK_WAIT_TIMEOUT | OperationalError | 8.4: HY000, 9.7: HY000 | lock wait timeout |
| 1206 | ER_LOCK_TABLE_FULL | OperationalError | 8.4: HY000, 9.7: HY000 | The server has insufficient memory for its lock table |
| 1207 | ER_READ_ONLY_TRANSACTION | ProgrammingError | 8.4: 25000, 9.7: 25000 | read only transaction |
| 1210 | ER_WRONG_ARGUMENTS | ProgrammingError | 8.4: HY000, 9.7: HY000 | invalid arguments |
| 1211 | ER_NO_PERMISSION_TO_CREATE_USER | OperationalError | 8.4: 42000, 9.7: 42000 | no permission to create user |
| 1213 | ER_LOCK_DEADLOCK | OperationalError | 8.4: 40001, 9.7: 40001 | lock deadlock |
| 1214 | ER_TABLE_CANT_HANDLE_FT | NotSupportedError | 8.4: HY000, 9.7: HY000 | The storage engine does not support FULLTEXT indexes |
| 1215 | ER_CANNOT_ADD_FOREIGN | DatabaseError | 8.4: HY000, 9.7: HY000 | A foreign key constraint could not be created |
| 1216 | ER_NO_REFERENCED_ROW | IntegrityError | 8.4: 23000, 9.7: 23000 | A child row has no matching referenced parent row |
| 1217 | ER_ROW_IS_REFERENCED | IntegrityError | 8.4: 23000, 9.7: 23000 | A parent row is referenced by child rows |
| 1218 | ER_CONNECT_TO_SOURCE | OperationalError | 8.4: 08S01, 9.7: 08S01 | A replica could not connect to its source |
| 1220 | ER_ERROR_WHEN_EXECUTING_COMMAND | DatabaseError | 8.4: HY000, 9.7: HY000 | The requested server command failed |
| 1221 | ER_WRONG_USAGE | ProgrammingError | 8.4: HY000, 9.7: HY000 | invalid usage |
| 1222 | ER_WRONG_NUMBER_OF_COLUMNS_IN_SELECT | ProgrammingError | 8.4: 21000, 9.7: 21000 | Combined SELECT statements return different column counts |
| 1223 | ER_CANT_UPDATE_WITH_READLOCK | ProgrammingError | 8.4: HY000, 9.7: HY000 | An update was attempted while the session holds a read lock |
| 1224 | ER_MIXING_NOT_ALLOWED | NotSupportedError | 8.4: HY000, 9.7: HY000 | The specified options cannot be combined |
| 1225 | ER_DUP_ARGUMENT | ProgrammingError | 8.4: HY000, 9.7: HY000 | An argument was specified more than once |
| 1226 | ER_USER_LIMIT_REACHED | OperationalError | 8.4: 42000, 9.7: 42000 | user limit reached |
| 1227 | ER_SPECIFIC_ACCESS_DENIED_ERROR | OperationalError | 8.4: 42000, 9.7: 42000 | The account lacks the privilege required for this operation |
| 1228 | ER_LOCAL_VARIABLE | ProgrammingError | 8.4: HY000, 9.7: HY000 | A session-only variable cannot be set with SET GLOBAL |
| 1229 | ER_GLOBAL_VARIABLE | ProgrammingError | 8.4: HY000, 9.7: HY000 | A global-only variable cannot be set with SET SESSION |
| 1230 | ER_NO_DEFAULT | ProgrammingError | 8.4: 42000, 9.7: 42000 | The variable has no default value |
| 1231 | ER_WRONG_VALUE_FOR_VAR | DataError | 8.4: 42000, 9.7: 42000 | invalid value for variable |
| 1232 | ER_WRONG_TYPE_FOR_VAR | DataError | 8.4: 42000, 9.7: 42000 | invalid type for variable |
| 1233 | ER_VAR_CANT_BE_READ | ProgrammingError | 8.4: HY000, 9.7: HY000 | variable cannot be read |
| 1234 | ER_CANT_USE_OPTION_HERE | ProgrammingError | 8.4: 42000, 9.7: 42000 | The option is not valid in this statement context |
| 1235 | ER_NOT_SUPPORTED_YET | NotSupportedError | 8.4: 42000, 9.7: 42000 | not supported yet |
| 1236 | ER_SOURCE_FATAL_ERROR_READING_BINLOG | OperationalError | 8.4: HY000, 9.7: HY000 | The replication source encountered a fatal error while reading its binary log |
| 1237 | ER_REPLICA_IGNORED_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | The replica ignored a table because of its replication filter |
| 1238 | ER_INCORRECT_GLOBAL_LOCAL_VAR | ProgrammingError | 8.4: HY000, 9.7: HY000 | The variable does not support the requested global or session access |
| 1239 | ER_WRONG_FK_DEF | ProgrammingError | 8.4: 42000, 9.7: 42000 | A foreign key definition is invalid |
| 1240 | ER_KEY_REF_DO_NOT_MATCH_TABLE_REF | InternalError | 8.4: HY000, 9.7: HY000 | The key reference and table reference do not agree |
| 1241 | ER_OPERAND_COLUMNS | ProgrammingError | 8.4: 21000, 9.7: 21000 | An operand has the wrong number of columns |
| 1242 | ER_SUBQUERY_NO_1_ROW | ProgrammingError | 8.4: 21000, 9.7: 21000 | A scalar subquery returned more than one row |
| 1243 | ER_UNKNOWN_STMT_HANDLER | ProgrammingError | 8.4: HY000, 9.7: HY000 | The prepared statement identifier is unknown |
| 1244 | ER_CORRUPT_HELP_DB | OperationalError | 8.4: HY000, 9.7: HY000 | corrupt help database |
| 1246 | ER_AUTO_CONVERT | DatabaseError | 8.4: HY000, 9.7: HY000 | The server converted one data type to another |
| 1247 | ER_ILLEGAL_REFERENCE | ProgrammingError | 8.4: 42S22, 9.7: 42S22 | A reference to a column or expression is not valid in this context |
| 1248 | ER_DERIVED_MUST_HAVE_ALIAS | ProgrammingError | 8.4: 42000, 9.7: 42000 | A derived table requires an alias |
| 1249 | ER_SELECT_REDUCED | DatabaseError | 8.4: 01000, 9.7: 01000 | The optimizer reduced a SELECT during query rewriting |
| 1250 | ER_TABLENAME_NOT_ALLOWED_HERE | ProgrammingError | 8.4: 42000, 9.7: 42000 | A table name is not permitted in this query clause |
| 1251 | ER_NOT_SUPPORTED_AUTH_MODE | NotSupportedError | 8.4: 08004, 9.7: 08004 | The client does not support the authentication protocol requested by the server |
| 1252 | ER_SPATIAL_CANT_HAVE_NULL | ProgrammingError | 8.4: 42000, 9.7: 42000 | A spatial index column must not permit NULL |
| 1253 | ER_COLLATION_CHARSET_MISMATCH | ProgrammingError | 8.4: 42000, 9.7: 42000 | collation character set mismatch |
| 1256 | ER_TOO_BIG_FOR_UNCOMPRESS | DataError | 8.4: HY000, 9.7: HY000 | The advertised uncompressed length exceeds the permitted limit |
| 1257 | ER_ZLIB_Z_MEM_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | Decompression failed because the compression library ran out of memory |
| 1258 | ER_ZLIB_Z_BUF_ERROR | DataError | 8.4: HY000, 9.7: HY000 | Decompression failed because the output buffer was too small |
| 1259 | ER_ZLIB_Z_DATA_ERROR | DataError | 8.4: HY000, 9.7: HY000 | The compressed input is invalid |
| 1260 | ER_CUT_VALUE_GROUP_CONCAT | DataError | 8.4: HY000, 9.7: HY000 | GROUP_CONCAT truncated its result to the configured maximum length |
| 1261 | ER_WARN_TOO_FEW_RECORDS | DataError | 8.4: 01000, 9.7: 01000 | An input record contains fewer values than the target column count |
| 1262 | ER_WARN_TOO_MANY_RECORDS | DataError | 8.4: 01000, 9.7: 01000 | An input record contains more values than the target column count |
| 1263 | ER_WARN_NULL_TO_NOTNULL | IntegrityError | 8.4: 22004, 9.7: 22004 | A NULL input was replaced for a column that does not allow NULL |
| 1264 | ER_WARN_DATA_OUT_OF_RANGE | DataError | 8.4: 22003, 9.7: 22003 | warn data out of range |
| 1265 | WARN_DATA_TRUNCATED | DataError | 8.4: 01000, 9.7: 01000 | data truncated |
| 1266 | ER_WARN_USING_OTHER_HANDLER | DatabaseError | 8.4: HY000, 9.7: HY000 | The server substituted a different storage engine |
| 1267 | ER_CANT_AGGREGATE_2COLLATIONS | ProgrammingError | 8.4: HY000, 9.7: HY000 | Two collations cannot be combined for the requested operation |
| 1269 | ER_REVOKE_GRANTS | OperationalError | 8.4: HY000, 9.7: HY000 | The server reports the account whose grants are being revoked |
| 1270 | ER_CANT_AGGREGATE_3COLLATIONS | ProgrammingError | 8.4: HY000, 9.7: HY000 | Three collations cannot be combined for the requested operation |
| 1271 | ER_CANT_AGGREGATE_NCOLLATIONS | ProgrammingError | 8.4: HY000, 9.7: HY000 | The specified collations cannot be combined for the requested operation |
| 1272 | ER_VARIABLE_IS_NOT_STRUCT | ProgrammingError | 8.4: HY000, 9.7: HY000 | The variable does not support structured member access |
| 1273 | ER_UNKNOWN_COLLATION | ProgrammingError | 8.4: HY000, 9.7: HY000 | unknown collation |
| 1274 | ER_REPLICA_IGNORED_SSL_PARAMS | DatabaseError | 8.4: HY000, 9.7: HY000 | The replica ignored SSL options because SSL support is unavailable |
| 1276 | ER_WARN_FIELD_RESOLVED | DatabaseError | 8.4: HY000, 9.7: HY000 | A column reference was resolved in an outer SELECT |
| 1277 | ER_BAD_REPLICA_UNTIL_COND | ProgrammingError | 8.4: HY000, 9.7: HY000 | The replica's UNTIL condition is invalid |
| 1278 | ER_MISSING_SKIP_REPLICA | DatabaseError | 8.4: HY000, 9.7: HY000 | Replication with START REPLICA UNTIL should disable automatic replica startup to preserve step-by-step control |
| 1279 | ER_UNTIL_COND_IGNORED | DatabaseError | 8.4: HY000, 9.7: HY000 | The UNTIL condition was ignored because the replica thread is not being started |
| 1280 | ER_WRONG_NAME_FOR_INDEX | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid name for index |
| 1281 | ER_WRONG_NAME_FOR_CATALOG | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid name for catalog |
| 1283 | ER_BAD_FT_COLUMN | ProgrammingError | 8.4: HY000, 9.7: HY000 | A FULLTEXT index column has an unsupported type |
| 1284 | ER_UNKNOWN_KEY_CACHE | ProgrammingError | 8.4: HY000, 9.7: HY000 | unknown key cache |
| 1285 | ER_WARN_HOSTNAME_WONT_WORK | OperationalError | 8.4: HY000, 9.7: HY000 | The host name cannot be used when name resolution is disabled |
| 1286 | ER_UNKNOWN_STORAGE_ENGINE | NotSupportedError | 8.4: 42000, 9.7: 42000 | unknown storage engine |
| 1287 | ER_WARN_DEPRECATED_SYNTAX | DatabaseError | 8.4: HY000, 9.7: HY000 | The statement uses syntax that is deprecated |
| 1288 | ER_NON_UPDATABLE_TABLE | ProgrammingError | 8.4: HY000, 9.7: HY000 | The target table or view cannot be updated |
| 1289 | ER_FEATURE_DISABLED | NotSupportedError | 8.4: HY000, 9.7: HY000 | feature disabled |
| 1290 | ER_OPTION_PREVENTS_STATEMENT | OperationalError | 8.4: HY000, 9.7: HY000 | option prevents statement |
| 1291 | ER_DUPLICATED_VALUE_IN_TYPE | ProgrammingError | 8.4: HY000, 9.7: HY000 | An ENUM or SET definition contains a duplicate member |
| 1292 | ER_TRUNCATED_WRONG_VALUE | DataError | 8.4: 22007, 9.7: 22007 | truncated invalid value |
| 1294 | ER_INVALID_ON_UPDATE | ProgrammingError | 8.4: HY000, 9.7: HY000 | The column's ON UPDATE clause is invalid |
| 1295 | ER_UNSUPPORTED_PS | NotSupportedError | 8.4: HY000, 9.7: HY000 | The statement is not supported by the prepared-statement protocol |
| 1296 | ER_GET_ERRMSG | DatabaseError | 8.4: HY000, 9.7: HY000 | The storage engine reported the error explained in the server message |
| 1297 | ER_GET_TEMPORARY_ERRMSG | OperationalError | 8.4: HY000, 9.7: HY000 | The storage engine reported a temporary error |
| 1298 | ER_UNKNOWN_TIME_ZONE | ProgrammingError | 8.4: HY000, 9.7: HY000 | unknown time zone |
| 1299 | ER_WARN_INVALID_TIMESTAMP | DataError | 8.4: HY000, 9.7: HY000 | A TIMESTAMP value was adjusted because it was invalid |
| 1300 | ER_INVALID_CHARACTER_STRING | DataError | 8.4: HY000, 9.7: HY000 | invalid character string |
| 1301 | ER_WARN_ALLOWED_PACKET_OVERFLOWED | DataError | 8.4: HY000, 9.7: HY000 | The result of a string function exceeds max_allowed_packet |
| 1302 | ER_CONFLICTING_DECLARATIONS | ProgrammingError | 8.4: HY000, 9.7: HY000 | conflicting declarations |
| 1303 | ER_SP_NO_RECURSIVE_CREATE | ProgrammingError | 8.4: 2F003, 9.7: 2F003 | A stored program cannot be created recursively |
| 1304 | ER_SP_ALREADY_EXISTS | ProgrammingError | 8.4: 42000, 9.7: 42000 | The stored procedure or function already exists |
| 1305 | ER_SP_DOES_NOT_EXIST | ProgrammingError | 8.4: 42000, 9.7: 42000 | The stored procedure or function does not exist |
| 1306 | ER_SP_DROP_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | The stored procedure or function could not be dropped |
| 1307 | ER_SP_STORE_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | The stored procedure or function could not be saved |
| 1308 | ER_SP_LILABEL_MISMATCH | ProgrammingError | 8.4: 42000, 9.7: 42000 | LEAVE or ITERATE refers to a label with no matching target |
| 1309 | ER_SP_LABEL_REDEFINE | ProgrammingError | 8.4: 42000, 9.7: 42000 | A stored program declares the same label more than once |
| 1310 | ER_SP_LABEL_MISMATCH | ProgrammingError | 8.4: 42000, 9.7: 42000 | A stored program's ending label does not match its opening label |
| 1311 | ER_SP_UNINIT_VAR | ProgrammingError | 8.4: 01000, 9.7: 01000 | A stored program uses an uninitialized variable |
| 1312 | ER_SP_BADSELECT | NotSupportedError | 8.4: 0A000, 9.7: 0A000 | The stored routine cannot return a result set in this calling context |
| 1313 | ER_SP_BADRETURN | ProgrammingError | 8.4: 42000, 9.7: 42000 | RETURN is used outside a stored function |
| 1314 | ER_SP_BADSTATEMENT | NotSupportedError | 8.4: 0A000, 9.7: 0A000 | The statement is not allowed inside this stored program |
| 1315 | ER_UPDATE_LOG_DEPRECATED_IGNORED | DatabaseError | 8.4: 42000, 9.7: 42000 | The deprecated update-log setting was ignored |
| 1316 | ER_UPDATE_LOG_DEPRECATED_TRANSLATED | DatabaseError | 8.4: 42000, 9.7: 42000 | The deprecated update-log setting was translated to binary logging |
| 1317 | ER_QUERY_INTERRUPTED | OperationalError | 8.4: 70100, 9.7: 70100 | query interrupted |
| 1318 | ER_SP_WRONG_NO_OF_ARGS | ProgrammingError | 8.4: 42000, 9.7: 42000 | A stored procedure or function was called with the wrong argument count |
| 1319 | ER_SP_COND_MISMATCH | ProgrammingError | 8.4: 42000, 9.7: 42000 | A stored program condition has incompatible definitions |
| 1320 | ER_SP_NORETURN | ProgrammingError | 8.4: 42000, 9.7: 42000 | A stored function contains no RETURN statement |
| 1321 | ER_SP_NORETURNEND | ProgrammingError | 8.4: 2F005, 9.7: 2F005 | A stored function reached its end without executing RETURN |
| 1322 | ER_SP_BAD_CURSOR_QUERY | ProgrammingError | 8.4: 42000, 9.7: 42000 | A cursor declaration cannot use this query |
| 1323 | ER_SP_BAD_CURSOR_SELECT | ProgrammingError | 8.4: 42000, 9.7: 42000 | A cursor query cannot contain SELECT INTO |
| 1324 | ER_SP_CURSOR_MISMATCH | ProgrammingError | 8.4: 42000, 9.7: 42000 | The stored program cursor is not declared |
| 1325 | ER_SP_CURSOR_ALREADY_OPEN | ProgrammingError | 8.4: 24000, 9.7: 24000 | The stored program cursor is already open |
| 1326 | ER_SP_CURSOR_NOT_OPEN | ProgrammingError | 8.4: 24000, 9.7: 24000 | The stored program cursor is not open |
| 1327 | ER_SP_UNDECLARED_VAR | ProgrammingError | 8.4: 42000, 9.7: 42000 | The stored program variable is not declared |
| 1328 | ER_SP_WRONG_NO_OF_FETCH_ARGS | ProgrammingError | 8.4: HY000, 9.7: HY000 | FETCH has the wrong number of destination variables |
| 1329 | ER_SP_FETCH_NO_DATA | DatabaseError | 8.4: 02000, 9.7: 02000 | A query or cursor fetch found no more rows |
| 1330 | ER_SP_DUP_PARAM | ProgrammingError | 8.4: 42000, 9.7: 42000 | A stored program parameter is declared more than once |
| 1331 | ER_SP_DUP_VAR | ProgrammingError | 8.4: 42000, 9.7: 42000 | A stored program variable is declared more than once |
| 1332 | ER_SP_DUP_COND | ProgrammingError | 8.4: 42000, 9.7: 42000 | A stored program condition is declared more than once |
| 1333 | ER_SP_DUP_CURS | ProgrammingError | 8.4: 42000, 9.7: 42000 | A stored program cursor is declared more than once |
| 1334 | ER_SP_CANT_ALTER | DatabaseError | 8.4: HY000, 9.7: HY000 | The stored procedure or function could not be altered |
| 1335 | ER_SP_SUBSELECT_NYI | NotSupportedError | 8.4: 0A000, 9.7: 0A000 | This subquery form is not supported in stored programs |
| 1336 | ER_STMT_NOT_ALLOWED_IN_SF_OR_TRG | NotSupportedError | 8.4: 0A000, 9.7: 0A000 | The statement is not allowed in a stored function or trigger |
| 1337 | ER_SP_VARCOND_AFTER_CURSHNDLR | ProgrammingError | 8.4: 42000, 9.7: 42000 | Local variables and conditions must be declared before cursors and handlers |
| 1338 | ER_SP_CURSOR_AFTER_HANDLER | ProgrammingError | 8.4: 42000, 9.7: 42000 | Cursors must be declared before handlers in a stored program |
| 1339 | ER_SP_CASE_NOT_FOUND | ProgrammingError | 8.4: 20000, 9.7: 20000 | No CASE branch matches and no ELSE branch is present |
| 1340 | ER_FPARSER_TOO_BIG_FILE | OperationalError | 8.4: HY000, 9.7: HY000 | A definition file exceeds the parser's supported size |
| 1341 | ER_FPARSER_BAD_HEADER | OperationalError | 8.4: HY000, 9.7: HY000 | A definition file has an invalid header |
| 1342 | ER_FPARSER_EOF_IN_COMMENT | OperationalError | 8.4: HY000, 9.7: HY000 | A definition file ends inside a comment |
| 1343 | ER_FPARSER_ERROR_IN_PARAMETER | OperationalError | 8.4: HY000, 9.7: HY000 | A definition file contains an invalid parameter |
| 1344 | ER_FPARSER_EOF_IN_UNKNOWN_PARAMETER | OperationalError | 8.4: HY000, 9.7: HY000 | A definition file ends inside an unknown parameter |
| 1345 | ER_VIEW_NO_EXPLAIN | OperationalError | 8.4: HY000, 9.7: HY000 | The account is not permitted to inspect the view definition |
| 1347 | ER_WRONG_OBJECT | ProgrammingError | 8.4: HY000, 9.7: HY000 | invalid object |
| 1348 | ER_NONUPDATEABLE_COLUMN | ProgrammingError | 8.4: HY000, 9.7: HY000 | nonupdateable column |
| 1350 | ER_VIEW_SELECT_CLAUSE | ProgrammingError | 8.4: HY000, 9.7: HY000 | A view definition uses a prohibited SELECT clause |
| 1351 | ER_VIEW_SELECT_VARIABLE | ProgrammingError | 8.4: HY000, 9.7: HY000 | A view definition uses a prohibited variable or parameter |
| 1352 | ER_VIEW_SELECT_TMPTABLE | ProgrammingError | 8.4: HY000, 9.7: HY000 | A view definition references a temporary table |
| 1353 | ER_VIEW_WRONG_LIST | ProgrammingError | 8.4: HY000, 9.7: HY000 | A view's declared column list does not match its SELECT |
| 1354 | ER_WARN_VIEW_MERGE | DatabaseError | 8.4: HY000, 9.7: HY000 | The server substituted another view algorithm because MERGE cannot be used |
| 1355 | ER_WARN_VIEW_WITHOUT_KEY | DatabaseError | 8.4: HY000, 9.7: HY000 | The view has no complete key from its underlying table |
| 1356 | ER_VIEW_INVALID | DatabaseError | 8.4: HY000, 9.7: HY000 | view invalid |
| 1357 | ER_SP_NO_DROP_SP | ProgrammingError | 8.4: HY000, 9.7: HY000 | A stored program cannot drop or alter another routine in this context |
| 1359 | ER_TRG_ALREADY_EXISTS | ProgrammingError | 8.4: HY000, 9.7: HY000 | The trigger already exists |
| 1360 | ER_TRG_DOES_NOT_EXIST | ProgrammingError | 8.4: HY000, 9.7: HY000 | The trigger does not exist |
| 1361 | ER_TRG_ON_VIEW_OR_TEMP_TABLE | ProgrammingError | 8.4: HY000, 9.7: HY000 | A trigger cannot be defined on a view or temporary table |
| 1362 | ER_TRG_CANT_CHANGE_ROW | ProgrammingError | 8.4: HY000, 9.7: HY000 | The trigger is not allowed to modify the specified NEW or OLD row value |
| 1363 | ER_TRG_NO_SUCH_ROW_IN_TRG | ProgrammingError | 8.4: HY000, 9.7: HY000 | The trigger has no NEW or OLD row in this event context |
| 1364 | ER_NO_DEFAULT_FOR_FIELD | IntegrityError | 8.4: HY000, 9.7: HY000 | no default for field |
| 1365 | ER_DIVISION_BY_ZERO | DataError | 8.4: 22012, 9.7: 22012 | division by zero |
| 1366 | ER_TRUNCATED_WRONG_VALUE_FOR_FIELD | DataError | 8.4: HY000, 9.7: HY000 | truncated invalid value for field |
| 1367 | ER_ILLEGAL_VALUE_FOR_TYPE | DataError | 8.4: 22007, 9.7: 22007 | A value is illegal for its declared type |
| 1368 | ER_VIEW_NONUPD_CHECK | ProgrammingError | 8.4: HY000, 9.7: HY000 | WITH CHECK OPTION cannot be used on a nonupdatable view |
| 1369 | ER_VIEW_CHECK_FAILED | IntegrityError | 8.4: HY000, 9.7: HY000 | view check failed |
| 1370 | ER_PROCACCESS_DENIED_ERROR | OperationalError | 8.4: 42000, 9.7: 42000 | The account lacks the required stored-routine privilege |
| 1371 | ER_RELAY_LOG_FAIL | OperationalError | 8.4: HY000, 9.7: HY000 | The replica's relay log could not be read or processed |
| 1373 | ER_UNKNOWN_TARGET_BINLOG | ProgrammingError | 8.4: HY000, 9.7: HY000 | The target binary log file was not found |
| 1374 | ER_IO_ERR_LOG_INDEX_READ | OperationalError | 8.4: HY000, 9.7: HY000 | Reading the binary log index failed |
| 1375 | ER_BINLOG_PURGE_PROHIBITED | OperationalError | 8.4: HY000, 9.7: HY000 | Binary log purging is prohibited in the current state |
| 1376 | ER_FSEEK_FAIL | OperationalError | 8.4: HY000, 9.7: HY000 | Seeking within a log file failed |
| 1377 | ER_BINLOG_PURGE_FATAL_ERR | OperationalError | 8.4: HY000, 9.7: HY000 | Purging a binary log encountered a fatal error |
| 1378 | ER_LOG_IN_USE | OperationalError | 8.4: HY000, 9.7: HY000 | log in use |
| 1379 | ER_LOG_PURGE_UNKNOWN_ERR | OperationalError | 8.4: HY000, 9.7: HY000 | Purging a log encountered an unspecified error |
| 1380 | ER_RELAY_LOG_INIT | OperationalError | 8.4: HY000, 9.7: HY000 | The relay log could not be initialized |
| 1381 | ER_NO_BINARY_LOGGING | OperationalError | 8.4: HY000, 9.7: HY000 | Binary logging is disabled |
| 1382 | ER_RESERVED_SYNTAX | ProgrammingError | 8.4: HY000, 9.7: HY000 | The statement uses reserved syntax |
| 1390 | ER_PS_MANY_PARAM | ProgrammingError | 8.4: HY000, 9.7: HY000 | A prepared statement has too many parameters |
| 1391 | ER_KEY_PART_0 | ProgrammingError | 8.4: HY000, 9.7: HY000 | An index key part has length zero |
| 1392 | ER_VIEW_CHECKSUM | InternalError | 8.4: HY000, 9.7: HY000 | The stored view definition has an invalid checksum |
| 1393 | ER_VIEW_MULTIUPDATE | ProgrammingError | 8.4: HY000, 9.7: HY000 | An update through a join view would modify more than one base table |
| 1394 | ER_VIEW_NO_INSERT_FIELD_LIST | ProgrammingError | 8.4: HY000, 9.7: HY000 | An insert into a view requires an explicit target column list |
| 1395 | ER_VIEW_DELETE_MERGE_VIEW | ProgrammingError | 8.4: HY000, 9.7: HY000 | The server cannot delete from this join view |
| 1396 | ER_CANNOT_USER | DatabaseError | 8.4: HY000, 9.7: HY000 | The account-management operation failed for the specified account |
| 1397 | ER_XAER_NOTA | ProgrammingError | 8.4: XAE04, 9.7: XAE04 | The distributed transaction identifier is unknown |
| 1398 | ER_XAER_INVAL | ProgrammingError | 8.4: XAE05, 9.7: XAE05 | A distributed transaction argument is invalid |
| 1399 | ER_XAER_RMFAIL | OperationalError | 8.4: XAE07, 9.7: XAE07 | The distributed transaction resource manager is in an invalid state |
| 1400 | ER_XAER_OUTSIDE | ProgrammingError | 8.4: XAE09, 9.7: XAE09 | Work is being performed outside the permitted distributed transaction context |
| 1401 | ER_XAER_RMERR | OperationalError | 8.4: XAE03, 9.7: XAE03 | The distributed transaction resource manager failed |
| 1402 | ER_XA_RBROLLBACK | OperationalError | 8.4: XA100, 9.7: XA100 | The distributed transaction was rolled back |
| 1403 | ER_NONEXISTING_PROC_GRANT | ProgrammingError | 8.4: 42000, 9.7: 42000 | The requested stored-routine grant does not exist |
| 1404 | ER_PROC_AUTO_GRANT_FAIL | OperationalError | 8.4: HY000, 9.7: HY000 | The server could not grant routine privileges automatically |
| 1405 | ER_PROC_AUTO_REVOKE_FAIL | OperationalError | 8.4: HY000, 9.7: HY000 | The server could not revoke routine privileges automatically |
| 1406 | ER_DATA_TOO_LONG | DataError | 8.4: 22001, 9.7: 22001 | data too long |
| 1407 | ER_SP_BAD_SQLSTATE | ProgrammingError | 8.4: 42000, 9.7: 42000 | A stored program specifies an invalid SQLSTATE |
| 1408 | ER_STARTUP | DatabaseError | 8.4: HY000, 9.7: HY000 | The server is starting up |
| 1409 | ER_LOAD_FROM_FIXED_SIZE_ROWS_TO_VAR | ProgrammingError | 8.4: HY000, 9.7: HY000 | Fixed-width input records cannot be loaded into this variable-width table |
| 1410 | ER_CANT_CREATE_USER_WITH_GRANT | ProgrammingError | 8.4: 42000, 9.7: 42000 | cannot create user with grant |
| 1411 | ER_WRONG_VALUE_FOR_TYPE | DataError | 8.4: HY000, 9.7: HY000 | invalid value for type |
| 1412 | ER_TABLE_DEF_CHANGED | OperationalError | 8.4: HY000, 9.7: HY000 | The table definition changed during the transaction |
| 1413 | ER_SP_DUP_HANDLER | ProgrammingError | 8.4: 42000, 9.7: 42000 | A stored program declares duplicate handlers |
| 1414 | ER_SP_NOT_VAR_ARG | ProgrammingError | 8.4: 42000, 9.7: 42000 | An OUT or INOUT argument must be a writable variable |
| 1415 | ER_SP_NO_RETSET | NotSupportedError | 8.4: 0A000, 9.7: 0A000 | The stored routine is not allowed to return a result set |
| 1416 | ER_CANT_CREATE_GEOMETRY_OBJECT | DataError | 8.4: 22003, 9.7: 22003 | cannot create geometry object |
| 1418 | ER_BINLOG_UNSAFE_ROUTINE | ProgrammingError | 8.4: HY000, 9.7: HY000 | The stored routine lacks required binary-logging safety characteristics |
| 1419 | ER_BINLOG_CREATE_ROUTINE_NEED_SUPER | OperationalError | 8.4: HY000, 9.7: HY000 | Creating this stored routine requires additional binary-logging privileges |
| 1421 | ER_STMT_HAS_NO_OPEN_CURSOR | ProgrammingError | 8.4: HY000, 9.7: HY000 | statement has no open cursor |
| 1422 | ER_COMMIT_NOT_ALLOWED_IN_SF_OR_TRG | NotSupportedError | 8.4: HY000, 9.7: HY000 | A commit or rollback is not permitted inside a stored function or trigger |
| 1423 | ER_NO_DEFAULT_FOR_VIEW_FIELD | IntegrityError | 8.4: HY000, 9.7: HY000 | The view's underlying column has no usable default value |
| 1424 | ER_SP_NO_RECURSION | NotSupportedError | 8.4: HY000, 9.7: HY000 | Stored functions and triggers do not support recursive invocation |
| 1425 | ER_TOO_BIG_SCALE | ProgrammingError | 8.4: 42000, 9.7: 42000 | A numeric scale exceeds its permitted maximum |
| 1426 | ER_TOO_BIG_PRECISION | ProgrammingError | 8.4: 42000, 9.7: 42000 | A numeric precision exceeds its permitted maximum |
| 1427 | ER_M_BIGGER_THAN_D | ProgrammingError | 8.4: 42000, 9.7: 42000 | A numeric scale exceeds the declared precision |
| 1428 | ER_WRONG_LOCK_OF_SYSTEM_TABLE | ProgrammingError | 8.4: HY000, 9.7: HY000 | invalid lock of system table |
| 1429 | ER_CONNECT_TO_FOREIGN_DATA_SOURCE | OperationalError | 8.4: HY000, 9.7: HY000 | Connecting to the foreign data source failed |
| 1430 | ER_QUERY_ON_FOREIGN_DATA_SOURCE | DatabaseError | 8.4: HY000, 9.7: HY000 | Executing a query on the foreign data source failed |
| 1431 | ER_FOREIGN_DATA_SOURCE_DOESNT_EXIST | ProgrammingError | 8.4: HY000, 9.7: HY000 | foreign data source doesnt exist |
| 1432 | ER_FOREIGN_DATA_STRING_INVALID_CANT_CREATE | ProgrammingError | 8.4: HY000, 9.7: HY000 | The foreign data source connection string is invalid for creation |
| 1433 | ER_FOREIGN_DATA_STRING_INVALID | ProgrammingError | 8.4: HY000, 9.7: HY000 | The foreign data source connection string is invalid |
| 1435 | ER_TRG_IN_WRONG_SCHEMA | ProgrammingError | 8.4: HY000, 9.7: HY000 | A trigger and its target table belong to different schemas |
| 1436 | ER_STACK_OVERRUN_NEED_MORE | OperationalError | 8.4: HY000, 9.7: HY000 | The server thread stack is too small for the operation |
| 1437 | ER_TOO_LONG_BODY | ProgrammingError | 8.4: 42000, 9.7: 42000 | too long body |
| 1438 | ER_WARN_CANT_DROP_DEFAULT_KEYCACHE | ProgrammingError | 8.4: HY000, 9.7: HY000 | The default key cache cannot be dropped |
| 1439 | ER_TOO_BIG_DISPLAYWIDTH | ProgrammingError | 8.4: 42000, 9.7: 42000 | A column's display width exceeds the permitted maximum |
| 1440 | ER_XAER_DUPID | ProgrammingError | 8.4: XAE08, 9.7: XAE08 | The distributed transaction identifier is already in use |
| 1441 | ER_DATETIME_FUNCTION_OVERFLOW | DataError | 8.4: 22008, 9.7: 22008 | datetime function overflow |
| 1442 | ER_CANT_UPDATE_USED_TABLE_IN_SF_OR_TRG | ProgrammingError | 8.4: HY000, 9.7: HY000 | A stored function or trigger cannot modify a table already used by its invoking statement |
| 1443 | ER_VIEW_PREVENT_UPDATE | ProgrammingError | 8.4: HY000, 9.7: HY000 | The view definition prevents the requested update |
| 1444 | ER_PS_NO_RECURSION | ProgrammingError | 8.4: HY000, 9.7: HY000 | A prepared statement cannot execute itself recursively |
| 1445 | ER_SP_CANT_SET_AUTOCOMMIT | ProgrammingError | 8.4: HY000, 9.7: HY000 | A stored function or trigger cannot change autocommit |
| 1447 | ER_VIEW_FRM_NO_USER | DatabaseError | 8.4: HY000, 9.7: HY000 | A legacy view has no defining account; the server uses the current account |
| 1448 | ER_VIEW_OTHER_USER | OperationalError | 8.4: HY000, 9.7: HY000 | The view belongs to another defining account |
| 1449 | ER_NO_SUCH_USER | OperationalError | 8.4: HY000, 9.7: HY000 | no such user |
| 1450 | ER_FORBID_SCHEMA_CHANGE | ProgrammingError | 8.4: HY000, 9.7: HY000 | The active stored program cannot switch to a different schema |
| 1451 | ER_ROW_IS_REFERENCED_2 | IntegrityError | 8.4: 23000, 9.7: 23000 | A parent row cannot be changed or deleted because child rows reference it |
| 1452 | ER_NO_REFERENCED_ROW_2 | IntegrityError | 8.4: 23000, 9.7: 23000 | A child row cannot be inserted or changed because its parent row is missing |
| 1453 | ER_SP_BAD_VAR_SHADOW | ProgrammingError | 8.4: 42000, 9.7: 42000 | A local declaration would shadow a stored program parameter |
| 1454 | ER_TRG_NO_DEFINER | ProgrammingError | 8.4: HY000, 9.7: HY000 | The trigger definition has no defining account |
| 1455 | ER_OLD_FILE_FORMAT | OperationalError | 8.4: HY000, 9.7: HY000 | old file format |
| 1456 | ER_SP_RECURSION_LIMIT | OperationalError | 8.4: HY000, 9.7: HY000 | The stored program recursion limit was exceeded |
| 1458 | ER_SP_WRONG_NAME | ProgrammingError | 8.4: 42000, 9.7: 42000 | A stored program name is invalid |
| 1459 | ER_TABLE_NEEDS_UPGRADE | OperationalError | 8.4: HY000, 9.7: HY000 | table needs upgrade |
| 1460 | ER_SP_NO_AGGREGATE | NotSupportedError | 8.4: 42000, 9.7: 42000 | An aggregate function is not allowed in the stored program |
| 1461 | ER_MAX_PREPARED_STMT_COUNT_REACHED | OperationalError | 8.4: 42000, 9.7: 42000 | server prepared-statement count limit reached |
| 1462 | ER_VIEW_RECURSIVE | ProgrammingError | 8.4: HY000, 9.7: HY000 | The view definition contains a recursive reference |
| 1463 | ER_NON_GROUPING_FIELD_USED | ProgrammingError | 8.4: 42000, 9.7: 42000 | A nongrouping column is used where a grouping column is required |
| 1464 | ER_TABLE_CANT_HANDLE_SPKEYS | NotSupportedError | 8.4: HY000, 9.7: HY000 | The storage engine does not support spatial indexes |
| 1465 | ER_NO_TRIGGERS_ON_SYSTEM_SCHEMA | ProgrammingError | 8.4: HY000, 9.7: HY000 | no triggers on system schema |
| 1466 | ER_REMOVED_SPACES | DatabaseError | 8.4: HY000, 9.7: HY000 | Leading spaces were removed from the user name |
| 1467 | ER_AUTOINC_READ_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | The storage engine could not obtain the next AUTO_INCREMENT value |
| 1468 | ER_USERNAME | DatabaseError | 8.4: HY000, 9.7: HY000 | The server message supplies a user-name fragment without an independent failure condition |
| 1469 | ER_HOSTNAME | DatabaseError | 8.4: HY000, 9.7: HY000 | The server message supplies a host-name fragment without an independent failure condition |
| 1470 | ER_WRONG_STRING_LENGTH | DataError | 8.4: HY000, 9.7: HY000 | invalid string length |
| 1471 | ER_NON_INSERTABLE_TABLE | ProgrammingError | 8.4: HY000, 9.7: HY000 | non insertable table |
| 1472 | ER_ADMIN_WRONG_MRG_TABLE | ProgrammingError | 8.4: HY000, 9.7: HY000 | A MERGE table definition is invalid |
| 1473 | ER_TOO_HIGH_LEVEL_OF_NESTING_FOR_SELECT | ProgrammingError | 8.4: HY000, 9.7: HY000 | too high level of nesting for select |
| 1474 | ER_NAME_BECOMES_EMPTY | ProgrammingError | 8.4: HY000, 9.7: HY000 | An identifier became empty after name processing |
| 1475 | ER_AMBIGUOUS_FIELD_TERM | ProgrammingError | 8.4: HY000, 9.7: HY000 | The field separator specification is ambiguous |
| 1476 | ER_FOREIGN_SERVER_EXISTS | ProgrammingError | 8.4: HY000, 9.7: HY000 | The foreign server definition already exists |
| 1477 | ER_FOREIGN_SERVER_DOESNT_EXIST | ProgrammingError | 8.4: HY000, 9.7: HY000 | The foreign server definition does not exist |
| 1478 | ER_ILLEGAL_HA_CREATE_OPTION | NotSupportedError | 8.4: HY000, 9.7: HY000 | The storage engine does not accept the specified table-creation option |
| 1479 | ER_PARTITION_REQUIRES_VALUES_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | The partition definition requires a VALUES clause |
| 1480 | ER_PARTITION_WRONG_VALUES_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | The partition definition has an invalid VALUES clause |
| 1481 | ER_PARTITION_MAXVALUE_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | MAXVALUE is used incorrectly in a partition definition |
| 1484 | ER_PARTITION_WRONG_NO_PART_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | The partition count is invalid |
| 1485 | ER_PARTITION_WRONG_NO_SUBPART_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | The subpartition count is invalid |
| 1486 | ER_WRONG_EXPR_IN_PARTITION_FUNC_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | The partition expression is invalid |
| 1488 | ER_FIELD_NOT_FOUND_PART_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | The partition expression refers to an unknown column |
| 1490 | ER_INCONSISTENT_PARTITION_INFO_ERROR | InternalError | 8.4: HY000, 9.7: HY000 | The partition definitions contain inconsistent metadata |
| 1491 | ER_PARTITION_FUNC_NOT_ALLOWED_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | The function is not allowed in a partition expression |
| 1492 | ER_PARTITIONS_MUST_BE_DEFINED_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | The table requires explicit partition definitions |
| 1493 | ER_RANGE_NOT_INCREASING_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | RANGE partition bounds are not strictly increasing |
| 1494 | ER_INCONSISTENT_TYPE_OF_FUNCTIONS_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | Partition functions use inconsistent types |
| 1495 | ER_MULTIPLE_DEF_CONST_IN_LIST_PART_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | A LIST partition value is specified more than once |
| 1496 | ER_PARTITION_ENTRY_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | The partition metadata contains an invalid entry |
| 1497 | ER_MIX_HANDLER_ERROR | NotSupportedError | 8.4: HY000, 9.7: HY000 | The partitioned table mixes incompatible storage engines |
| 1498 | ER_PARTITION_NOT_DEFINED_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | The referenced partition is not defined |
| 1499 | ER_TOO_MANY_PARTITIONS_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | too many partitions error |
| 1500 | ER_SUBPARTITION_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | The subpartition definition is invalid |
| 1501 | ER_CANT_CREATE_HANDLER_FILE | OperationalError | 8.4: HY000, 9.7: HY000 | cannot create handler file |
| 1502 | ER_BLOB_FIELD_IN_PART_FUNC_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | A BLOB or TEXT column cannot be used by the partitioning function |
| 1503 | ER_UNIQUE_KEY_NEED_ALL_FIELDS_IN_PF | ProgrammingError | 8.4: HY000, 9.7: HY000 | Every unique key must contain all columns used by the partitioning expression |
| 1504 | ER_NO_PARTS_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | The partition set is empty |
| 1505 | ER_PARTITION_MGMT_ON_NONPARTITIONED | ProgrammingError | 8.4: HY000, 9.7: HY000 | A partition-management operation was requested for a nonpartitioned table |
| 1506 | ER_FOREIGN_KEY_ON_PARTITIONED | NotSupportedError | 8.4: HY000, 9.7: HY000 | Foreign keys are not supported on the partitioned table |
| 1507 | ER_DROP_PARTITION_NON_EXISTENT | ProgrammingError | 8.4: HY000, 9.7: HY000 | The partition to be dropped does not exist |
| 1508 | ER_DROP_LAST_PARTITION | ProgrammingError | 8.4: HY000, 9.7: HY000 | The table's last remaining partition cannot be dropped |
| 1509 | ER_COALESCE_ONLY_ON_HASH_PARTITION | ProgrammingError | 8.4: HY000, 9.7: HY000 | COALESCE PARTITION is permitted only for HASH or KEY partitioning |
| 1510 | ER_REORG_HASH_ONLY_ON_SAME_NO | ProgrammingError | 8.4: HY000, 9.7: HY000 | Reorganizing HASH partitions must preserve the partition count |
| 1511 | ER_REORG_NO_PARAM_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | REORGANIZE PARTITION has invalid arguments |
| 1512 | ER_ONLY_ON_RANGE_LIST_PARTITION | ProgrammingError | 8.4: HY000, 9.7: HY000 | The operation is permitted only for RANGE or LIST partitions |
| 1513 | ER_ADD_PARTITION_SUBPART_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | A subpartition cannot be added independently of its partition |
| 1514 | ER_ADD_PARTITION_NO_NEW_PARTITION | ProgrammingError | 8.4: HY000, 9.7: HY000 | ADD PARTITION specifies no new partition |
| 1515 | ER_COALESCE_PARTITION_NO_PARTITION | ProgrammingError | 8.4: HY000, 9.7: HY000 | COALESCE PARTITION specifies no partition count |
| 1516 | ER_REORG_PARTITION_NOT_EXIST | ProgrammingError | 8.4: HY000, 9.7: HY000 | The partition to be reorganized does not exist |
| 1517 | ER_SAME_NAME_PARTITION | ProgrammingError | 8.4: HY000, 9.7: HY000 | Partition names must be unique |
| 1518 | ER_NO_BINLOG_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | The binary log is unavailable |
| 1519 | ER_CONSECUTIVE_REORG_PARTITIONS | ProgrammingError | 8.4: HY000, 9.7: HY000 | The partitions being reorganized must be consecutive |
| 1520 | ER_REORG_OUTSIDE_RANGE | ProgrammingError | 8.4: HY000, 9.7: HY000 | The reorganized partitions do not preserve the required range |
| 1521 | ER_PARTITION_FUNCTION_FAILURE | NotSupportedError | 8.4: HY000, 9.7: HY000 | Evaluating the partitioning function failed |
| 1523 | ER_LIMITED_PART_RANGE | NotSupportedError | 8.4: HY000, 9.7: HY000 | The partition range exceeds its supported limit |
| 1524 | ER_PLUGIN_IS_NOT_LOADED | NotSupportedError | 8.4: HY000, 9.7: HY000 | plugin is not loaded |
| 1525 | ER_WRONG_VALUE | DataError | 8.4: HY000, 9.7: HY000 | invalid value |
| 1526 | ER_NO_PARTITION_FOR_GIVEN_VALUE | DataError | 8.4: HY000, 9.7: HY000 | no partition for given value |
| 1527 | ER_FILEGROUP_OPTION_ONLY_ONCE | ProgrammingError | 8.4: HY000, 9.7: HY000 | The filegroup option is specified more than once |
| 1528 | ER_CREATE_FILEGROUP_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | create filegroup failed |
| 1529 | ER_DROP_FILEGROUP_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | drop filegroup failed |
| 1530 | ER_TABLESPACE_AUTO_EXTEND_ERROR | NotSupportedError | 8.4: HY000, 9.7: HY000 | tablespace auto extend error |
| 1531 | ER_WRONG_SIZE_NUMBER | DataError | 8.4: HY000, 9.7: HY000 | A size specification is invalid |
| 1532 | ER_SIZE_OVERFLOW_ERROR | DataError | 8.4: HY000, 9.7: HY000 | A size specification overflows its supported range |
| 1533 | ER_ALTER_FILEGROUP_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | alter filegroup failed |
| 1534 | ER_BINLOG_ROW_LOGGING_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | Writing a row event to the binary log failed |
| 1537 | ER_EVENT_ALREADY_EXISTS | ProgrammingError | 8.4: HY000, 9.7: HY000 | event already exists |
| 1539 | ER_EVENT_DOES_NOT_EXIST | ProgrammingError | 8.4: HY000, 9.7: HY000 | event does not exist |
| 1542 | ER_EVENT_INTERVAL_NOT_POSITIVE_OR_TOO_BIG | DataError | 8.4: HY000, 9.7: HY000 | The event interval must be positive and within the supported range |
| 1543 | ER_EVENT_ENDS_BEFORE_STARTS | ProgrammingError | 8.4: HY000, 9.7: HY000 | An event's end precedes its start |
| 1544 | ER_EVENT_EXEC_TIME_IN_THE_PAST | DatabaseError | 8.4: HY000, 9.7: HY000 | The event's scheduled execution time is in the past |
| 1551 | ER_EVENT_SAME_NAME | ProgrammingError | 8.4: HY000, 9.7: HY000 | event same name |
| 1553 | ER_DROP_INDEX_FK | IntegrityError | 8.4: HY000, 9.7: HY000 | An index required by a foreign key cannot be dropped |
| 1554 | ER_WARN_DEPRECATED_SYNTAX_WITH_VER | DatabaseError | 8.4: HY000, 9.7: HY000 | The deprecated syntax has the version restriction described by the server message |
| 1556 | ER_CANT_LOCK_LOG_TABLE | ProgrammingError | 8.4: HY000, 9.7: HY000 | cannot lock log table |
| 1557 | ER_FOREIGN_DUPLICATE_KEY_OLD_UNUSED | IntegrityError | 8.4: 23000, 9.7: 23000 | A foreign key operation would duplicate a unique key |
| 1558 | ER_COL_COUNT_DOESNT_MATCH_PLEASE_UPDATE | OperationalError | 8.4: HY000, 9.7: HY000 | A system table's column count does not match the server's expected definition |
| 1560 | ER_STORED_FUNCTION_PREVENTS_SWITCH_BINLOG_FORMAT | ProgrammingError | 8.4: HY000, 9.7: HY000 | A stored function prevents changing the binary log format |
| 1562 | ER_PARTITION_NO_TEMPORARY | NotSupportedError | 8.4: HY000, 9.7: HY000 | A partitioned table cannot be temporary |
| 1563 | ER_PARTITION_CONST_DOMAIN_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | A partition bound is outside its column's value domain |
| 1564 | ER_PARTITION_FUNCTION_IS_NOT_ALLOWED | ProgrammingError | 8.4: HY000, 9.7: HY000 | The function is not allowed in a partition expression |
| 1566 | ER_NULL_IN_VALUES_LESS_THAN | ProgrammingError | 8.4: HY000, 9.7: HY000 | NULL is not allowed in a VALUES LESS THAN partition bound |
| 1567 | ER_WRONG_PARTITION_NAME | ProgrammingError | 8.4: HY000, 9.7: HY000 | invalid partition name |
| 1568 | ER_CANT_CHANGE_TX_CHARACTERISTICS | ProgrammingError | 8.4: 25001, 9.7: 25001 | cannot change transaction characteristics |
| 1569 | ER_DUP_ENTRY_AUTOINCREMENT_CASE | IntegrityError | 8.4: HY000, 9.7: HY000 | An AUTO_INCREMENT value would duplicate an existing key |
| 1571 | ER_EVENT_SET_VAR_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | The event scheduler could not apply a system variable setting |
| 1572 | ER_PARTITION_MERGE_ERROR | NotSupportedError | 8.4: HY000, 9.7: HY000 | Merging the partitions failed |
| 1575 | ER_BASE64_DECODE_ERROR | DataError | 8.4: HY000, 9.7: HY000 | base64 decode error |
| 1576 | ER_EVENT_RECURSION_FORBIDDEN | ProgrammingError | 8.4: HY000, 9.7: HY000 | An event cannot recursively invoke event scheduling |
| 1578 | ER_ONLY_INTEGERS_ALLOWED | ProgrammingError | 8.4: HY000, 9.7: HY000 | only integers allowed |
| 1579 | ER_UNSUPORTED_LOG_ENGINE | NotSupportedError | 8.4: HY000, 9.7: HY000 | This storage engine is not supported for log tables |
| 1580 | ER_BAD_LOG_STATEMENT | ProgrammingError | 8.4: HY000, 9.7: HY000 | The binary log statement or its arguments are invalid |
| 1581 | ER_CANT_RENAME_LOG_TABLE | ProgrammingError | 8.4: HY000, 9.7: HY000 | cannot rename log table |
| 1582 | ER_WRONG_PARAMCOUNT_TO_NATIVE_FCT | ProgrammingError | 8.4: 42000, 9.7: 42000 | A built-in function was called with the wrong argument count |
| 1583 | ER_WRONG_PARAMETERS_TO_NATIVE_FCT | ProgrammingError | 8.4: 42000, 9.7: 42000 | A built-in function was called with invalid arguments |
| 1584 | ER_WRONG_PARAMETERS_TO_STORED_FCT | ProgrammingError | 8.4: 42000, 9.7: 42000 | A stored function was called with invalid arguments |
| 1585 | ER_NATIVE_FCT_NAME_COLLISION | ProgrammingError | 8.4: HY000, 9.7: HY000 | A stored function name conflicts with a built-in function name |
| 1586 | ER_DUP_ENTRY_WITH_KEY_NAME | IntegrityError | 8.4: 23000, 9.7: 23000 | A row would duplicate the named unique key |
| 1587 | ER_BINLOG_PURGE_EMFILE | OperationalError | 8.4: HY000, 9.7: HY000 | Purging binary logs exhausted the available file descriptors |
| 1588 | ER_EVENT_CANNOT_CREATE_IN_THE_PAST | DatabaseError | 8.4: HY000, 9.7: HY000 | The expired event was created and then removed because its execution time is already past |
| 1589 | ER_EVENT_CANNOT_ALTER_IN_THE_PAST | ProgrammingError | 8.4: HY000, 9.7: HY000 | An event cannot be altered to this execution time in the past |
| 1591 | ER_NO_PARTITION_FOR_GIVEN_VALUE_SILENT | DataError | 8.4: HY000, 9.7: HY000 | No partition accepts the supplied value |
| 1592 | ER_BINLOG_UNSAFE_STATEMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | The statement is unsafe for statement-based replication |
| 1593 | ER_BINLOG_FATAL_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | A fatal binary log failure occurred |
| 1598 | ER_BINLOG_LOGGING_IMPOSSIBLE | OperationalError | 8.4: HY000, 9.7: HY000 | The statement cannot be logged with the current binary log configuration |
| 1599 | ER_VIEW_NO_CREATION_CTX | OperationalError | 8.4: HY000, 9.7: HY000 | The view definition lacks its creation context |
| 1600 | ER_VIEW_INVALID_CREATION_CTX | OperationalError | 8.4: HY000, 9.7: HY000 | The view's stored creation context is invalid |
| 1602 | ER_TRG_CORRUPTED_FILE | InternalError | 8.4: HY000, 9.7: HY000 | The trigger definition file is corrupt |
| 1603 | ER_TRG_NO_CREATION_CTX | OperationalError | 8.4: HY000, 9.7: HY000 | The trigger definition lacks its creation context |
| 1604 | ER_TRG_INVALID_CREATION_CTX | OperationalError | 8.4: HY000, 9.7: HY000 | The trigger's stored creation context is invalid |
| 1605 | ER_EVENT_INVALID_CREATION_CTX | OperationalError | 8.4: HY000, 9.7: HY000 | The event's stored creation context is invalid |
| 1606 | ER_TRG_CANT_OPEN_TABLE | OperationalError | 8.4: HY000, 9.7: HY000 | The trigger's target table could not be opened |
| 1609 | ER_NO_FORMAT_DESCRIPTION_EVENT_BEFORE_BINLOG_STATEMENT | ProgrammingError | 8.4: HY000, 9.7: HY000 | A BINLOG statement requires a preceding format-description event |
| 1610 | ER_REPLICA_CORRUPT_EVENT | InternalError | 8.4: HY000, 9.7: HY000 | The replica received a corrupt replication event |
| 1612 | ER_LOG_PURGE_NO_FILE | OperationalError | 8.4: HY000, 9.7: HY000 | The log file to be purged does not exist |
| 1613 | ER_XA_RBTIMEOUT | OperationalError | 8.4: XA106, 9.7: XA106 | The distributed transaction was rolled back after a timeout |
| 1614 | ER_XA_RBDEADLOCK | OperationalError | 8.4: XA102, 9.7: XA102 | The distributed transaction was rolled back after a deadlock |
| 1615 | ER_NEED_REPREPARE | OperationalError | 8.4: HY000, 9.7: HY000 | The prepared statement must be prepared again |
| 1617 | WARN_NO_CONNECTION_METADATA | DatabaseError | 8.4: HY000, 9.7: HY000 | Replication connection metadata is missing |
| 1618 | WARN_OPTION_IGNORED | DatabaseError | 8.4: HY000, 9.7: HY000 | option ignored |
| 1619 | ER_PLUGIN_DELETE_BUILTIN | ProgrammingError | 8.4: HY000, 9.7: HY000 | A built-in plugin cannot be uninstalled |
| 1620 | WARN_PLUGIN_BUSY | DatabaseError | 8.4: HY000, 9.7: HY000 | plugin busy |
| 1621 | ER_VARIABLE_IS_READONLY | ProgrammingError | 8.4: HY000, 9.7: HY000 | variable is readonly |
| 1622 | ER_WARN_ENGINE_TRANSACTION_ROLLBACK | OperationalError | 8.4: HY000, 9.7: HY000 | The storage engine rolled back the transaction |
| 1624 | ER_REPLICA_HEARTBEAT_VALUE_OUT_OF_RANGE | DataError | 8.4: HY000, 9.7: HY000 | replica heartbeat value out of range |
| 1625 | ER_NDB_REPLICATION_SCHEMA_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | The NDB replication metadata schema is invalid |
| 1626 | ER_CONFLICT_FN_PARSE_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | The replication conflict-resolution function specification is invalid |
| 1627 | ER_EXCEPTIONS_WRITE_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | The replication exception row could not be written |
| 1628 | ER_TOO_LONG_TABLE_COMMENT | ProgrammingError | 8.4: HY000, 9.7: HY000 | too long table comment |
| 1629 | ER_TOO_LONG_FIELD_COMMENT | ProgrammingError | 8.4: HY000, 9.7: HY000 | A column comment exceeds its maximum length |
| 1630 | ER_FUNC_INEXISTENT_NAME_COLLISION | ProgrammingError | 8.4: 42000, 9.7: 42000 | The requested function cannot be found under the server's function-name resolution rules |
| 1631 | ER_DATABASE_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | The server message supplies a database-name fragment without an independent failure condition |
| 1632 | ER_TABLE_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | The server message supplies a table-name fragment without an independent failure condition |
| 1633 | ER_PARTITION_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | The server message supplies a partition-name fragment without an independent failure condition |
| 1634 | ER_SUBPARTITION_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | The server message supplies a subpartition-name fragment without an independent failure condition |
| 1635 | ER_TEMPORARY_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | The server message supplies a temporary-object name fragment without an independent failure condition |
| 1636 | ER_RENAMED_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | The server message supplies a renamed-object fragment without an independent failure condition |
| 1637 | ER_TOO_MANY_CONCURRENT_TRXS | OperationalError | 8.4: HY000, 9.7: HY000 | The maximum number of concurrent transactions has been reached |
| 1638 | WARN_NON_ASCII_SEPARATOR_NOT_IMPLEMENTED | NotSupportedError | 8.4: HY000, 9.7: HY000 | Non-ASCII field or line separators are not supported in this operation |
| 1639 | ER_DEBUG_SYNC_TIMEOUT | OperationalError | 8.4: HY000, 9.7: HY000 | debug sync timeout |
| 1640 | ER_DEBUG_SYNC_HIT_LIMIT | OperationalError | 8.4: HY000, 9.7: HY000 | debug sync hit limit |
| 1641 | ER_DUP_SIGNAL_SET | ProgrammingError | 8.4: 42000, 9.7: 42000 | A SIGNAL information item is assigned more than once |
| 1642 | ER_SIGNAL_WARN | DatabaseError | 8.4: 01000, 9.7: 01000 | A stored program raised a warning with SIGNAL |
| 1643 | ER_SIGNAL_NOT_FOUND | DatabaseError | 8.4: 02000, 9.7: 02000 | A stored program raised a no-data condition with SIGNAL |
| 1644 | ER_SIGNAL_EXCEPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | A stored program raised an exception with SIGNAL |
| 1645 | ER_RESIGNAL_WITHOUT_ACTIVE_HANDLER | ProgrammingError | 8.4: 0K000, 9.7: 0K000 | resignal without active handler |
| 1646 | ER_SIGNAL_BAD_CONDITION_TYPE | ProgrammingError | 8.4: HY000, 9.7: HY000 | SIGNAL requires a valid SQLSTATE condition |
| 1647 | WARN_COND_ITEM_TRUNCATED | DataError | 8.4: HY000, 9.7: HY000 | A diagnostics information item was truncated |
| 1648 | ER_COND_ITEM_TOO_LONG | DataError | 8.4: HY000, 9.7: HY000 | A diagnostics information item exceeds its maximum length |
| 1649 | ER_UNKNOWN_LOCALE | ProgrammingError | 8.4: HY000, 9.7: HY000 | unknown locale |
| 1650 | ER_REPLICA_IGNORE_SERVER_IDS | ProgrammingError | 8.4: HY000, 9.7: HY000 | The replica's ignored-server identifier configuration is invalid |
| 1652 | ER_SAME_NAME_PARTITION_FIELD | ProgrammingError | 8.4: HY000, 9.7: HY000 | A partition column is specified more than once |
| 1653 | ER_PARTITION_COLUMN_LIST_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | The partition column list is invalid |
| 1654 | ER_WRONG_TYPE_COLUMN_VALUE_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | A partition value has an incompatible type |
| 1655 | ER_TOO_MANY_PARTITION_FUNC_FIELDS_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | The partitioning expression uses too many columns |
| 1656 | ER_MAXVALUE_IN_VALUES_IN | ProgrammingError | 8.4: HY000, 9.7: HY000 | MAXVALUE is not allowed in VALUES IN |
| 1657 | ER_TOO_MANY_VALUES_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | A partition value list has too many values |
| 1658 | ER_ROW_SINGLE_PARTITION_FIELD_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | A row constructor is invalid for a single partition column |
| 1659 | ER_FIELD_TYPE_NOT_ALLOWED_AS_PARTITION_FIELD | ProgrammingError | 8.4: HY000, 9.7: HY000 | field type not allowed as partition field |
| 1660 | ER_PARTITION_FIELDS_TOO_LONG | ProgrammingError | 8.4: HY000, 9.7: HY000 | The combined partition column lengths exceed the supported limit |
| 1661 | ER_BINLOG_ROW_ENGINE_AND_STMT_ENGINE | NotSupportedError | 8.4: HY000, 9.7: HY000 | The statement mixes storage engines with incompatible row and statement logging requirements |
| 1662 | ER_BINLOG_ROW_MODE_AND_STMT_ENGINE | NotSupportedError | 8.4: HY000, 9.7: HY000 | Row-based binary logging cannot log an operation on a statement-only storage engine |
| 1663 | ER_BINLOG_UNSAFE_AND_STMT_ENGINE | NotSupportedError | 8.4: HY000, 9.7: HY000 | An unsafe statement cannot be logged using this statement-only storage engine |
| 1664 | ER_BINLOG_ROW_INJECTION_AND_STMT_ENGINE | NotSupportedError | 8.4: HY000, 9.7: HY000 | An injected row event cannot be applied to a statement-only storage engine |
| 1665 | ER_BINLOG_STMT_MODE_AND_ROW_ENGINE | NotSupportedError | 8.4: HY000, 9.7: HY000 | Statement-based binary logging cannot log an operation on a row-only storage engine |
| 1666 | ER_BINLOG_ROW_INJECTION_AND_STMT_MODE | NotSupportedError | 8.4: HY000, 9.7: HY000 | An injected row event cannot be logged in statement-based format |
| 1667 | ER_BINLOG_MULTIPLE_ENGINES_AND_SELF_LOGGING_ENGINE | NotSupportedError | 8.4: HY000, 9.7: HY000 | The statement mixes multiple engines with an incompatible self-logging engine |
| 1668 | ER_BINLOG_UNSAFE_LIMIT | DatabaseError | 8.4: HY000, 9.7: HY000 | A statement with LIMIT is unsafe for statement-based replication |
| 1670 | ER_BINLOG_UNSAFE_SYSTEM_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | A statement using a system table is unsafe for statement-based replication |
| 1671 | ER_BINLOG_UNSAFE_AUTOINC_COLUMNS | DatabaseError | 8.4: HY000, 9.7: HY000 | An AUTO_INCREMENT operation is unsafe for statement-based replication |
| 1672 | ER_BINLOG_UNSAFE_UDF | DatabaseError | 8.4: HY000, 9.7: HY000 | A statement using a loadable function is unsafe for statement-based replication |
| 1673 | ER_BINLOG_UNSAFE_SYSTEM_VARIABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | A statement using a system variable is unsafe for statement-based replication |
| 1674 | ER_BINLOG_UNSAFE_SYSTEM_FUNCTION | DatabaseError | 8.4: HY000, 9.7: HY000 | A statement using a system function is unsafe for statement-based replication |
| 1675 | ER_BINLOG_UNSAFE_NONTRANS_AFTER_TRANS | DatabaseError | 8.4: HY000, 9.7: HY000 | A nontransactional update after a transactional update is unsafe for statement-based replication |
| 1676 | ER_MESSAGE_AND_STATEMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | The server reports an error together with the affected statement |
| 1678 | ER_REPLICA_CANT_CREATE_CONVERSION | OperationalError | 8.4: HY000, 9.7: HY000 | The replica could not create the required column-conversion metadata |
| 1679 | ER_INSIDE_TRANSACTION_PREVENTS_SWITCH_BINLOG_FORMAT | ProgrammingError | 8.4: HY000, 9.7: HY000 | The binary log format cannot be changed inside an active transaction |
| 1680 | ER_PATH_LENGTH | ProgrammingError | 8.4: HY000, 9.7: HY000 | A filesystem path exceeds its supported length |
| 1681 | ER_WARN_DEPRECATED_SYNTAX_NO_REPLACEMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | The statement uses deprecated syntax with no replacement |
| 1682 | ER_WRONG_NATIVE_TABLE_STRUCTURE | OperationalError | 8.4: HY000, 9.7: HY000 | A native system table has an invalid definition |
| 1683 | ER_WRONG_PERFSCHEMA_USAGE | ProgrammingError | 8.4: HY000, 9.7: HY000 | The requested Performance Schema operation is invalid |
| 1684 | ER_WARN_I_S_SKIPPED_TABLE | OperationalError | 8.4: HY000, 9.7: HY000 | An INFORMATION_SCHEMA table was skipped |
| 1685 | ER_INSIDE_TRANSACTION_PREVENTS_SWITCH_BINLOG_DIRECT | ProgrammingError | 8.4: HY000, 9.7: HY000 | Direct logging of nontransactional updates cannot be changed inside a transaction |
| 1686 | ER_STORED_FUNCTION_PREVENTS_SWITCH_BINLOG_DIRECT | ProgrammingError | 8.4: HY000, 9.7: HY000 | Direct logging of nontransactional updates cannot be changed inside a stored function |
| 1687 | ER_SPATIAL_MUST_HAVE_GEOM_COL | ProgrammingError | 8.4: 42000, 9.7: 42000 | A spatial index must include a geometry column |
| 1688 | ER_TOO_LONG_INDEX_COMMENT | ProgrammingError | 8.4: HY000, 9.7: HY000 | too long index comment |
| 1689 | ER_LOCK_ABORTED | OperationalError | 8.4: HY000, 9.7: HY000 | lock aborted |
| 1690 | ER_DATA_OUT_OF_RANGE | DataError | 8.4: 22003, 9.7: 22003 | data out of range |
| 1692 | ER_BINLOG_UNSAFE_MULTIPLE_ENGINES_AND_SELF_LOGGING_ENGINE | DatabaseError | 8.4: HY000, 9.7: HY000 | A statement mixing a self-logging engine and other engines is unsafe for replication |
| 1693 | ER_BINLOG_UNSAFE_MIXED_STATEMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | The statement is unsafe under mixed binary logging |
| 1694 | ER_INSIDE_TRANSACTION_PREVENTS_SWITCH_SQL_LOG_BIN | ProgrammingError | 8.4: HY000, 9.7: HY000 | Binary logging cannot be enabled or disabled inside a transaction |
| 1695 | ER_STORED_FUNCTION_PREVENTS_SWITCH_SQL_LOG_BIN | ProgrammingError | 8.4: HY000, 9.7: HY000 | Binary logging cannot be enabled or disabled inside a stored function |
| 1696 | ER_FAILED_READ_FROM_PAR_FILE | OperationalError | 8.4: HY000, 9.7: HY000 | The table's partition metadata file could not be read |
| 1697 | ER_VALUES_IS_NOT_INT_TYPE_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | A partition VALUES expression must be an integer |
| 1698 | ER_ACCESS_DENIED_NO_PASSWORD_ERROR | OperationalError | 8.4: 28000, 9.7: 28000 | Access was denied for an account authenticating without a password |
| 1701 | ER_TRUNCATE_ILLEGAL_FK | IntegrityError | 8.4: 42000, 9.7: 42000 | TRUNCATE is prohibited because a foreign key references the table |
| 1702 | ER_PLUGIN_IS_PERMANENT | OperationalError | 8.4: HY000, 9.7: HY000 | plugin is permanent |
| 1703 | ER_REPLICA_HEARTBEAT_VALUE_OUT_OF_RANGE_MIN | DataError | 8.4: HY000, 9.7: HY000 | The replication heartbeat interval is below its supported minimum |
| 1704 | ER_REPLICA_HEARTBEAT_VALUE_OUT_OF_RANGE_MAX | DataError | 8.4: HY000, 9.7: HY000 | The replication heartbeat interval is above its supported maximum |
| 1705 | ER_STMT_CACHE_FULL | OperationalError | 8.4: HY000, 9.7: HY000 | statement cache full |
| 1706 | ER_MULTI_UPDATE_KEY_CONFLICT | ProgrammingError | 8.4: HY000, 9.7: HY000 | A multi-table update would update the same key through conflicting aliases |
| 1707 | ER_TABLE_NEEDS_REBUILD | OperationalError | 8.4: HY000, 9.7: HY000 | table needs rebuild |
| 1708 | WARN_OPTION_BELOW_LIMIT | DataError | 8.4: HY000, 9.7: HY000 | An option value is below its supported minimum |
| 1709 | ER_INDEX_COLUMN_TOO_LONG | ProgrammingError | 8.4: HY000, 9.7: HY000 | index column too long |
| 1710 | ER_ERROR_IN_TRIGGER_BODY | DatabaseError | 8.4: HY000, 9.7: HY000 | The named trigger body contains an error |
| 1711 | ER_ERROR_IN_UNKNOWN_TRIGGER_BODY | DatabaseError | 8.4: HY000, 9.7: HY000 | An unidentified trigger body contains an error |
| 1712 | ER_INDEX_CORRUPT | InternalError | 8.4: HY000, 9.7: HY000 | index corrupt |
| 1713 | ER_UNDO_RECORD_TOO_BIG | DataError | 8.4: HY000, 9.7: HY000 | undo record too big |
| 1714 | ER_BINLOG_UNSAFE_INSERT_IGNORE_SELECT | DatabaseError | 8.4: HY000, 9.7: HY000 | INSERT IGNORE ... SELECT is unsafe for statement-based replication |
| 1715 | ER_BINLOG_UNSAFE_INSERT_SELECT_UPDATE | DatabaseError | 8.4: HY000, 9.7: HY000 | INSERT ... SELECT ... ON DUPLICATE KEY UPDATE is unsafe for statement-based replication |
| 1716 | ER_BINLOG_UNSAFE_REPLACE_SELECT | DatabaseError | 8.4: HY000, 9.7: HY000 | REPLACE ... SELECT is unsafe for statement-based replication |
| 1717 | ER_BINLOG_UNSAFE_CREATE_IGNORE_SELECT | DatabaseError | 8.4: HY000, 9.7: HY000 | CREATE ... IGNORE ... SELECT is unsafe for statement-based replication |
| 1718 | ER_BINLOG_UNSAFE_CREATE_REPLACE_SELECT | DatabaseError | 8.4: HY000, 9.7: HY000 | CREATE ... REPLACE ... SELECT is unsafe for statement-based replication |
| 1719 | ER_BINLOG_UNSAFE_UPDATE_IGNORE | DatabaseError | 8.4: HY000, 9.7: HY000 | UPDATE IGNORE is unsafe for statement-based replication |
| 1720 | ER_PLUGIN_NO_UNINSTALL | NotSupportedError | 8.4: HY000, 9.7: HY000 | The plugin does not permit uninstallation |
| 1721 | ER_PLUGIN_NO_INSTALL | NotSupportedError | 8.4: HY000, 9.7: HY000 | The plugin does not permit installation |
| 1722 | ER_BINLOG_UNSAFE_WRITE_AUTOINC_SELECT | DatabaseError | 8.4: HY000, 9.7: HY000 | Writing AUTO_INCREMENT values using SELECT is unsafe for statement-based replication |
| 1723 | ER_BINLOG_UNSAFE_CREATE_SELECT_AUTOINC | DatabaseError | 8.4: HY000, 9.7: HY000 | Creating AUTO_INCREMENT values with CREATE ... SELECT is unsafe for statement-based replication |
| 1724 | ER_BINLOG_UNSAFE_INSERT_TWO_KEYS | DatabaseError | 8.4: HY000, 9.7: HY000 | An insert with multiple unique keys is unsafe for statement-based replication |
| 1725 | ER_TABLE_IN_FK_CHECK | OperationalError | 8.4: HY000, 9.7: HY000 | The table is in use by a foreign key check |
| 1726 | ER_UNSUPPORTED_ENGINE | NotSupportedError | 8.4: HY000, 9.7: HY000 | unsupported engine |
| 1727 | ER_BINLOG_UNSAFE_AUTOINC_NOT_FIRST | DatabaseError | 8.4: HY000, 9.7: HY000 | The AUTO_INCREMENT column is not first in its key and is unsafe for statement-based replication |
| 1728 | ER_CANNOT_LOAD_FROM_TABLE_V2 | OperationalError | 8.4: HY000, 9.7: HY000 | A required system table has an incompatible definition |
| 1729 | ER_SOURCE_DELAY_VALUE_OUT_OF_RANGE | DataError | 8.4: HY000, 9.7: HY000 | The replication delay is outside its supported range |
| 1730 | ER_ONLY_FD_AND_RBR_EVENTS_ALLOWED_IN_BINLOG_STATEMENT | ProgrammingError | 8.4: HY000, 9.7: HY000 | A BINLOG statement permits only format-description and row-based events |
| 1731 | ER_PARTITION_EXCHANGE_DIFFERENT_OPTION | ProgrammingError | 8.4: HY000, 9.7: HY000 | The partition and exchange table have different table options |
| 1732 | ER_PARTITION_EXCHANGE_PART_TABLE | ProgrammingError | 8.4: HY000, 9.7: HY000 | The exchange table must not itself be partitioned |
| 1733 | ER_PARTITION_EXCHANGE_TEMP_TABLE | ProgrammingError | 8.4: HY000, 9.7: HY000 | The exchange table must not be temporary |
| 1734 | ER_PARTITION_INSTEAD_OF_SUBPARTITION | ProgrammingError | 8.4: HY000, 9.7: HY000 | The operation requires a subpartition rather than a partition |
| 1735 | ER_UNKNOWN_PARTITION | ProgrammingError | 8.4: HY000, 9.7: HY000 | unknown partition |
| 1736 | ER_TABLES_DIFFERENT_METADATA | ProgrammingError | 8.4: HY000, 9.7: HY000 | The tables have incompatible metadata |
| 1737 | ER_ROW_DOES_NOT_MATCH_PARTITION | DataError | 8.4: HY000, 9.7: HY000 | row does not match partition |
| 1738 | ER_BINLOG_CACHE_SIZE_GREATER_THAN_MAX | DatabaseError | 8.4: HY000, 9.7: HY000 | The server adjusted the binary log cache size to its configured maximum |
| 1739 | ER_WARN_INDEX_NOT_APPLICABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | The requested index cannot be used for this comparison |
| 1740 | ER_PARTITION_EXCHANGE_FOREIGN_KEY | ProgrammingError | 8.4: HY000, 9.7: HY000 | A table with foreign keys cannot be used in this partition exchange |
| 1742 | ER_RPL_INFO_DATA_TOO_LONG | DataError | 8.4: HY000, 9.7: HY000 | Replication metadata exceeds its supported length |
| 1745 | ER_BINLOG_STMT_CACHE_SIZE_GREATER_THAN_MAX | DatabaseError | 8.4: HY000, 9.7: HY000 | The server adjusted the binary log statement cache size to its configured maximum |
| 1746 | ER_CANT_UPDATE_TABLE_IN_CREATE_TABLE_SELECT | ProgrammingError | 8.4: HY000, 9.7: HY000 | CREATE TABLE ... SELECT cannot update a table it is using |
| 1747 | ER_PARTITION_CLAUSE_ON_NONPARTITIONED | ProgrammingError | 8.4: HY000, 9.7: HY000 | A PARTITION clause was used for a nonpartitioned table |
| 1748 | ER_ROW_DOES_NOT_MATCH_GIVEN_PARTITION_SET | DataError | 8.4: HY000, 9.7: HY000 | row does not match given partition set |
| 1750 | ER_CHANGE_RPL_INFO_REPOSITORY_FAILURE | OperationalError | 8.4: HY000, 9.7: HY000 | The replication metadata repository could not be changed |
| 1751 | ER_WARNING_NOT_COMPLETE_ROLLBACK_WITH_CREATED_TEMP_TABLE | OperationalError | 8.4: HY000, 9.7: HY000 | Rollback cannot undo creation of temporary tables |
| 1752 | ER_WARNING_NOT_COMPLETE_ROLLBACK_WITH_DROPPED_TEMP_TABLE | OperationalError | 8.4: HY000, 9.7: HY000 | Rollback cannot restore dropped temporary tables |
| 1753 | ER_MTA_FEATURE_IS_NOT_SUPPORTED | NotSupportedError | 8.4: HY000, 9.7: HY000 | The parallel replication applier does not support this operation |
| 1754 | ER_MTA_UPDATED_DBS_GREATER_MAX | DatabaseError | 8.4: HY000, 9.7: HY000 | The replication event omits database-name metadata because too many databases are involved |
| 1755 | ER_MTA_CANT_PARALLEL | DatabaseError | 8.4: HY000, 9.7: HY000 | The replication event cannot be applied in parallel |
| 1756 | ER_MTA_INCONSISTENT_DATA | DatabaseError | 8.4: HY000, 9.7: HY000 | The parallel replication applier detected inconsistent data |
| 1757 | ER_FULLTEXT_NOT_SUPPORTED_WITH_PARTITIONING | NotSupportedError | 8.4: HY000, 9.7: HY000 | fulltext not supported with partitioning |
| 1758 | ER_DA_INVALID_CONDITION_NUMBER | ProgrammingError | 8.4: 35000, 9.7: 35000 | GET DIAGNOSTICS references an invalid condition number |
| 1759 | ER_INSECURE_PLAIN_TEXT | DatabaseError | 8.4: HY000, 9.7: HY000 | The operation exposes a password as clear text |
| 1760 | ER_INSECURE_CHANGE_SOURCE | DatabaseError | 8.4: HY000, 9.7: HY000 | Changing the replication source would use insecure connection settings |
| 1761 | ER_FOREIGN_DUPLICATE_KEY_WITH_CHILD_INFO | IntegrityError | 8.4: 23000, 9.7: 23000 | A foreign key operation would duplicate a unique key; the message identifies the child row |
| 1762 | ER_FOREIGN_DUPLICATE_KEY_WITHOUT_CHILD_INFO | IntegrityError | 8.4: 23000, 9.7: 23000 | A foreign key operation would duplicate a unique key without identifying the child row |
| 1763 | ER_SQLTHREAD_WITH_SECURE_REPLICA | ProgrammingError | 8.4: HY000, 9.7: HY000 | Replication authentication options cannot be applied to a SQL-thread-only start |
| 1764 | ER_TABLE_HAS_NO_FT | ProgrammingError | 8.4: HY000, 9.7: HY000 | The table has no FULLTEXT index |
| 1765 | ER_VARIABLE_NOT_SETTABLE_IN_SF_OR_TRIGGER | ProgrammingError | 8.4: HY000, 9.7: HY000 | The variable cannot be set inside a stored function or trigger |
| 1766 | ER_VARIABLE_NOT_SETTABLE_IN_TRANSACTION | ProgrammingError | 8.4: HY000, 9.7: HY000 | The variable cannot be set inside a transaction |
| 1769 | ER_SET_STATEMENT_CANNOT_INVOKE_FUNCTION | ProgrammingError | 8.4: HY000, 9.7: HY000 | A SET statement cannot invoke this function |
| 1770 | ER_GTID_NEXT_CANT_BE_AUTOMATIC_IF_GTID_NEXT_LIST_IS_NON_NULL | ProgrammingError | 8.4: HY000, 9.7: HY000 | GTID_NEXT cannot be AUTOMATIC while GTID_NEXT_LIST is non-NULL |
| 1772 | ER_MALFORMED_GTID_SET_SPECIFICATION | DataError | 8.4: HY000, 9.7: HY000 | malformed GTID set specification |
| 1773 | ER_MALFORMED_GTID_SET_ENCODING | DataError | 8.4: HY000, 9.7: HY000 | malformed GTID set encoding |
| 1774 | ER_MALFORMED_GTID_SPECIFICATION | DataError | 8.4: HY000, 9.7: HY000 | malformed GTID specification |
| 1775 | ER_GNO_EXHAUSTED | OperationalError | 8.4: HY000, 9.7: HY000 | The global transaction number space is exhausted |
| 1776 | ER_BAD_REPLICA_AUTO_POSITION | ProgrammingError | 8.4: HY000, 9.7: HY000 | The replication auto-positioning configuration is invalid |
| 1777 | ER_AUTO_POSITION_REQUIRES_GTID_MODE_NOT_OFF | OperationalError | 8.4: HY000, 9.7: HY000 | Replication auto-positioning requires global transaction identifier mode |
| 1778 | ER_CANT_DO_IMPLICIT_COMMIT_IN_TRX_WHEN_GTID_NEXT_IS_SET | ProgrammingError | 8.4: HY000, 9.7: HY000 | An implicit-commit statement cannot run inside a transaction with an assigned global identifier |
| 1779 | ER_GTID_MODE_ON_REQUIRES_ENFORCE_GTID_CONSISTENCY_ON | ProgrammingError | 8.4: HY000, 9.7: HY000 | Enabling global transaction identifiers requires consistency enforcement |
| 1781 | ER_CANT_SET_GTID_NEXT_TO_GTID_WHEN_GTID_MODE_IS_OFF | ProgrammingError | 8.4: HY000, 9.7: HY000 | GTID_NEXT cannot specify a transaction identifier while identifier mode is disabled |
| 1782 | ER_CANT_SET_GTID_NEXT_TO_ANONYMOUS_WHEN_GTID_MODE_IS_ON | ProgrammingError | 8.4: HY000, 9.7: HY000 | GTID_NEXT cannot be ANONYMOUS while identifier mode is enabled |
| 1783 | ER_CANT_SET_GTID_NEXT_LIST_TO_NON_NULL_WHEN_GTID_MODE_IS_OFF | ProgrammingError | 8.4: HY000, 9.7: HY000 | GTID_NEXT_LIST must be NULL while identifier mode is disabled |
| 1785 | ER_GTID_UNSAFE_NON_TRANSACTIONAL_TABLE | ProgrammingError | 8.4: HY000, 9.7: HY000 | Updating the nontransactional table would violate global transaction identifier consistency |
| 1786 | ER_GTID_UNSAFE_CREATE_SELECT | NotSupportedError | 8.4: HY000, 9.7: HY000 | CREATE TABLE ... SELECT would violate global transaction identifier consistency |
| 1788 | ER_GTID_MODE_CAN_ONLY_CHANGE_ONE_STEP_AT_A_TIME | ProgrammingError | 8.4: HY000, 9.7: HY000 | Global transaction identifier mode can change only through adjacent supported states |
| 1789 | ER_SOURCE_HAS_PURGED_REQUIRED_GTIDS | OperationalError | 8.4: HY000, 9.7: HY000 | The source has purged transaction identifiers required by the replica |
| 1790 | ER_CANT_SET_GTID_NEXT_WHEN_OWNING_GTID | ProgrammingError | 8.4: HY000, 9.7: HY000 | A session cannot change GTID_NEXT while it owns a transaction identifier |
| 1791 | ER_UNKNOWN_EXPLAIN_FORMAT | ProgrammingError | 8.4: HY000, 9.7: HY000 | unknown explain format |
| 1792 | ER_CANT_EXECUTE_IN_READ_ONLY_TRANSACTION | ProgrammingError | 8.4: 25006, 9.7: 25006 | cannot execute in read only transaction |
| 1793 | ER_TOO_LONG_TABLE_PARTITION_COMMENT | ProgrammingError | 8.4: HY000, 9.7: HY000 | too long table partition comment |
| 1794 | ER_REPLICA_CONFIGURATION | OperationalError | 8.4: HY000, 9.7: HY000 | The replica configuration is invalid |
| 1795 | ER_INNODB_FT_LIMIT | NotSupportedError | 8.4: HY000, 9.7: HY000 | InnoDB permits only one FULLTEXT index creation in this operation |
| 1796 | ER_INNODB_NO_FT_TEMP_TABLE | NotSupportedError | 8.4: HY000, 9.7: HY000 | InnoDB FULLTEXT indexes cannot be created on temporary tables |
| 1797 | ER_INNODB_FT_WRONG_DOCID_COLUMN | ProgrammingError | 8.4: HY000, 9.7: HY000 | The InnoDB FULLTEXT document identifier column is invalid |
| 1798 | ER_INNODB_FT_WRONG_DOCID_INDEX | ProgrammingError | 8.4: HY000, 9.7: HY000 | The InnoDB FULLTEXT document identifier index is invalid |
| 1799 | ER_INNODB_ONLINE_LOG_TOO_BIG | OperationalError | 8.4: HY000, 9.7: HY000 | The InnoDB online ALTER log exceeds its configured maximum |
| 1800 | ER_UNKNOWN_ALTER_ALGORITHM | ProgrammingError | 8.4: HY000, 9.7: HY000 | unknown alter algorithm |
| 1801 | ER_UNKNOWN_ALTER_LOCK | ProgrammingError | 8.4: HY000, 9.7: HY000 | unknown alter lock |
| 1802 | ER_MTA_CHANGE_SOURCE_CANT_RUN_WITH_GAPS | OperationalError | 8.4: HY000, 9.7: HY000 | The replication source cannot be changed while parallel-applier gaps remain |
| 1803 | ER_MTA_RECOVERY_FAILURE | OperationalError | 8.4: HY000, 9.7: HY000 | Parallel replication recovery failed |
| 1804 | ER_MTA_RESET_WORKERS | OperationalError | 8.4: HY000, 9.7: HY000 | The replication worker metadata could not be reset |
| 1805 | ER_COL_COUNT_DOESNT_MATCH_CORRUPTED_V2 | OperationalError | 8.4: HY000, 9.7: HY000 | A system table has an incompatible column count and may be corrupt |
| 1806 | ER_REPLICA_SILENT_RETRY_TRANSACTION | OperationalError | 8.4: HY000, 9.7: HY000 | The replica transaction requires an internal retry |
| 1807 | ER_DISCARD_FK_CHECKS_RUNNING | OperationalError | 8.4: HY000, 9.7: HY000 | A tablespace cannot be discarded while foreign key checks are running |
| 1808 | ER_TABLE_SCHEMA_MISMATCH | ProgrammingError | 8.4: HY000, 9.7: HY000 | table schema mismatch |
| 1809 | ER_TABLE_IN_SYSTEM_TABLESPACE | NotSupportedError | 8.4: HY000, 9.7: HY000 | table in system tablespace |
| 1810 | ER_IO_READ_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | I/O read error |
| 1811 | ER_IO_WRITE_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | I/O write error |
| 1812 | ER_TABLESPACE_MISSING | OperationalError | 8.4: HY000, 9.7: HY000 | tablespace missing |
| 1813 | ER_TABLESPACE_EXISTS | ProgrammingError | 8.4: HY000, 9.7: HY000 | tablespace exists |
| 1814 | ER_TABLESPACE_DISCARDED | OperationalError | 8.4: HY000, 9.7: HY000 | tablespace discarded |
| 1815 | ER_INTERNAL_ERROR | InternalError | 8.4: HY000, 9.7: HY000 | internal error |
| 1816 | ER_INNODB_IMPORT_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | Importing an InnoDB tablespace failed |
| 1817 | ER_INNODB_INDEX_CORRUPT | InternalError | 8.4: HY000, 9.7: HY000 | An InnoDB index is corrupt |
| 1818 | ER_INVALID_YEAR_COLUMN_LENGTH | ProgrammingError | 8.4: HY000, 9.7: HY000 | The declared YEAR column length is invalid |
| 1819 | ER_NOT_VALID_PASSWORD | DataError | 8.4: HY000, 9.7: HY000 | not valid password |
| 1820 | ER_MUST_CHANGE_PASSWORD | OperationalError | 8.4: HY000, 9.7: HY000 | must change password |
| 1821 | ER_FK_NO_INDEX_CHILD | ProgrammingError | 8.4: HY000, 9.7: HY000 | The child table lacks a required foreign key index |
| 1822 | ER_FK_NO_INDEX_PARENT | ProgrammingError | 8.4: HY000, 9.7: HY000 | The parent table lacks a required referenced index |
| 1823 | ER_FK_FAIL_ADD_SYSTEM | NotSupportedError | 8.4: HY000, 9.7: HY000 | The system could not register the foreign key constraint |
| 1824 | ER_FK_CANNOT_OPEN_PARENT | DatabaseError | 8.4: HY000, 9.7: HY000 | The referenced parent table could not be opened |
| 1825 | ER_FK_INCORRECT_OPTION | ProgrammingError | 8.4: HY000, 9.7: HY000 | A foreign key option is invalid |
| 1826 | ER_FK_DUP_NAME | ProgrammingError | 8.4: HY000, 9.7: HY000 | The foreign key constraint name is already in use |
| 1827 | ER_PASSWORD_FORMAT | DataError | 8.4: HY000, 9.7: HY000 | The supplied password hash has an invalid format |
| 1828 | ER_FK_COLUMN_CANNOT_DROP | IntegrityError | 8.4: HY000, 9.7: HY000 | A column used by a foreign key cannot be dropped |
| 1829 | ER_FK_COLUMN_CANNOT_DROP_CHILD | IntegrityError | 8.4: HY000, 9.7: HY000 | A column referenced by a child table's foreign key cannot be dropped |
| 1830 | ER_FK_COLUMN_NOT_NULL | ProgrammingError | 8.4: HY000, 9.7: HY000 | A foreign key action requires a nullable column but the column is NOT NULL |
| 1831 | ER_DUP_INDEX | DatabaseError | 8.4: HY000, 9.7: HY000 | An equivalent index already exists |
| 1832 | ER_FK_COLUMN_CANNOT_CHANGE | IntegrityError | 8.4: HY000, 9.7: HY000 | A column used by a foreign key cannot be changed in this way |
| 1833 | ER_FK_COLUMN_CANNOT_CHANGE_CHILD | IntegrityError | 8.4: HY000, 9.7: HY000 | A column referenced by a child table's foreign key cannot be changed in this way |
| 1835 | ER_MALFORMED_PACKET | OperationalError | 8.4: HY000, 9.7: HY000 | malformed packet |
| 1836 | ER_READ_ONLY_MODE | OperationalError | 8.4: HY000, 9.7: HY000 | read only mode |
| 1837 | ER_GTID_NEXT_TYPE_UNDEFINED_GTID | ProgrammingError | 8.4: HY000, 9.7: HY000 | GTID_NEXT has an undefined value for the next transaction |
| 1838 | ER_VARIABLE_NOT_SETTABLE_IN_SP | ProgrammingError | 8.4: HY000, 9.7: HY000 | The variable cannot be set inside a stored procedure |
| 1840 | ER_CANT_SET_GTID_PURGED_WHEN_GTID_EXECUTED_IS_NOT_EMPTY | ProgrammingError | 8.4: HY000, 9.7: HY000 | GTID_PURGED cannot be changed in this way while GTID_EXECUTED is nonempty |
| 1841 | ER_CANT_SET_GTID_PURGED_WHEN_OWNED_GTIDS_IS_NOT_EMPTY | OperationalError | 8.4: HY000, 9.7: HY000 | GTID_PURGED cannot be changed while active transactions own identifiers |
| 1842 | ER_GTID_PURGED_WAS_CHANGED | DatabaseError | 8.4: HY000, 9.7: HY000 | The server reports a change to GTID_PURGED |
| 1843 | ER_GTID_EXECUTED_WAS_CHANGED | DatabaseError | 8.4: HY000, 9.7: HY000 | The server reports a change to GTID_EXECUTED |
| 1844 | ER_BINLOG_STMT_MODE_AND_NO_REPL_TABLES | NotSupportedError | 8.4: HY000, 9.7: HY000 | Statement-based logging cannot handle the involved nonreplicated tables |
| 1845 | ER_ALTER_OPERATION_NOT_SUPPORTED | NotSupportedError | 8.4: 0A000, 9.7: 0A000 | alter operation not supported |
| 1846 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON | NotSupportedError | 8.4: 0A000, 9.7: 0A000 | The requested ALTER TABLE operation is unsupported for the reason in the message |
| 1847 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_COPY | NotSupportedError | 8.4: HY000, 9.7: HY000 | The ALTER TABLE operation requires the COPY algorithm |
| 1848 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_PARTITION | NotSupportedError | 8.4: HY000, 9.7: HY000 | The requested ALTER TABLE operation is unsupported for this partitioned table |
| 1849 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_FK_RENAME | NotSupportedError | 8.4: HY000, 9.7: HY000 | The ALTER TABLE operation would require renaming a foreign key |
| 1850 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_COLUMN_TYPE | NotSupportedError | 8.4: HY000, 9.7: HY000 | The ALTER TABLE algorithm cannot perform this column type change |
| 1851 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_FK_CHECK | NotSupportedError | 8.4: HY000, 9.7: HY000 | The ALTER TABLE algorithm cannot perform the required foreign key checks |
| 1853 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_NOPK | ProgrammingError | 8.4: HY000, 9.7: HY000 | The ALTER TABLE algorithm requires a primary key |
| 1854 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_AUTOINC | NotSupportedError | 8.4: HY000, 9.7: HY000 | The ALTER TABLE algorithm cannot handle this AUTO_INCREMENT change |
| 1855 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_HIDDEN_FTS | NotSupportedError | 8.4: HY000, 9.7: HY000 | The ALTER TABLE operation conflicts with a hidden FULLTEXT column |
| 1856 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_CHANGE_FTS | NotSupportedError | 8.4: HY000, 9.7: HY000 | The ALTER TABLE algorithm cannot change this FULLTEXT definition |
| 1857 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_FTS | NotSupportedError | 8.4: HY000, 9.7: HY000 | The ALTER TABLE algorithm cannot perform this FULLTEXT operation |
| 1859 | ER_DUP_UNKNOWN_IN_INDEX | IntegrityError | 8.4: 23000, 9.7: 23000 | An index would contain duplicate values whose full representation is unavailable |
| 1860 | ER_IDENT_CAUSES_TOO_LONG_PATH | ProgrammingError | 8.4: HY000, 9.7: HY000 | An identifier would produce an excessively long filesystem path |
| 1861 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_NOT_NULL | DataError | 8.4: HY000, 9.7: HY000 | The ALTER TABLE algorithm cannot make the requested NOT NULL change |
| 1862 | ER_MUST_CHANGE_PASSWORD_LOGIN | OperationalError | 8.4: HY000, 9.7: HY000 | The account's password must be changed before login can complete |
| 1863 | ER_ROW_IN_WRONG_PARTITION | InternalError | 8.4: HY000, 9.7: HY000 | A table row is stored in the wrong partition |
| 1864 | ER_MTA_EVENT_BIGGER_PENDING_JOBS_SIZE_MAX | OperationalError | 8.4: HY000, 9.7: HY000 | A parallel replication event exceeds the pending-jobs memory limit |
| 1866 | ER_BINLOG_LOGICAL_CORRUPTION | InternalError | 8.4: HY000, 9.7: HY000 | binlog logical corruption |
| 1867 | ER_WARN_PURGE_LOG_IN_USE | OperationalError | 8.4: HY000, 9.7: HY000 | A binary log could not be purged because it is in use |
| 1868 | ER_WARN_PURGE_LOG_IS_ACTIVE | OperationalError | 8.4: HY000, 9.7: HY000 | The active binary log cannot be purged |
| 1869 | ER_AUTO_INCREMENT_CONFLICT | IntegrityError | 8.4: HY000, 9.7: HY000 | auto increment conflict |
| 1870 | WARN_ON_BLOCKHOLE_IN_RBR | DatabaseError | 8.4: HY000, 9.7: HY000 | A row-based change to a BLACKHOLE table cannot be logged as required |
| 1871 | ER_REPLICA_CM_INIT_REPOSITORY | OperationalError | 8.4: HY000, 9.7: HY000 | The replica coordinator's metadata repository could not be initialized |
| 1872 | ER_REPLICA_AM_INIT_REPOSITORY | OperationalError | 8.4: HY000, 9.7: HY000 | The replica applier's metadata repository could not be initialized |
| 1873 | ER_ACCESS_DENIED_CHANGE_USER_ERROR | OperationalError | 8.4: 28000, 9.7: 28000 | Access was denied while changing the authenticated account |
| 1874 | ER_INNODB_READ_ONLY | OperationalError | 8.4: HY000, 9.7: HY000 | innodb read only |
| 1875 | ER_STOP_REPLICA_SQL_THREAD_TIMEOUT | OperationalError | 8.4: HY000, 9.7: HY000 | Stopping the replication SQL thread timed out |
| 1876 | ER_STOP_REPLICA_IO_THREAD_TIMEOUT | OperationalError | 8.4: HY000, 9.7: HY000 | Stopping the replication I/O thread timed out |
| 1877 | ER_TABLE_CORRUPT | DatabaseError | 8.4: HY000, 9.7: HY000 | table corrupt |
| 1878 | ER_TEMP_FILE_WRITE_FAILURE | OperationalError | 8.4: HY000, 9.7: HY000 | temp file write failure |
| 1879 | ER_INNODB_FT_AUX_NOT_HEX_ID | OperationalError | 8.4: HY000, 9.7: HY000 | The InnoDB FULLTEXT auxiliary table identifier is not a valid hexadecimal identifier |
| 1880 | ER_OLD_TEMPORALS_UPGRADED | DatabaseError | 8.4: HY000, 9.7: HY000 | Legacy temporal columns were upgraded to the current format |
| 1881 | ER_INNODB_FORCED_RECOVERY | OperationalError | 8.4: HY000, 9.7: HY000 | innodb forced recovery |
| 1882 | ER_AES_INVALID_IV | DataError | 8.4: HY000, 9.7: HY000 | The AES initialization vector is invalid |
| 1883 | ER_PLUGIN_CANNOT_BE_UNINSTALLED | OperationalError | 8.4: HY000, 9.7: HY000 | plugin cannot be uninstalled |
| 1884 | ER_GTID_UNSAFE_BINLOG_SPLITTABLE_STATEMENT_AND_ASSIGNED_GTID | ProgrammingError | 8.4: HY000, 9.7: HY000 | A statement that must be split across binary log transactions cannot use one assigned global identifier |
| 1885 | ER_REPLICA_HAS_MORE_GTIDS_THAN_SOURCE | OperationalError | 8.4: HY000, 9.7: HY000 | replica has more gtids than source |
| 1886 | ER_MISSING_KEY | ProgrammingError | 8.4: HY000, 9.7: HY000 | The table lacks an index required by the requested operation |
| 1887 | WARN_NAMED_PIPE_ACCESS_EVERYONE | DatabaseError | 8.4: HY000, 9.7: HY000 | A Windows named pipe allows overly broad access |
| 3000 | ER_FILE_CORRUPT | DatabaseError | 8.4: HY000, 9.7: HY000 | file corrupt |
| 3001 | ER_ERROR_ON_SOURCE | OperationalError | 8.4: HY000, 9.7: HY000 | The replication source reported an error |
| 3003 | ER_STORAGE_ENGINE_NOT_LOADED | NotSupportedError | 8.4: HY000, 9.7: HY000 | storage engine not loaded |
| 3004 | ER_GET_STACKED_DA_WITHOUT_ACTIVE_HANDLER | ProgrammingError | 8.4: 0Z002, 9.7: 0Z002 | GET STACKED DIAGNOSTICS requires an active condition handler |
| 3005 | ER_WARN_LEGACY_SYNTAX_CONVERTED | DatabaseError | 8.4: HY000, 9.7: HY000 | The server converted legacy syntax to its current equivalent |
| 3006 | ER_BINLOG_UNSAFE_FULLTEXT_PLUGIN | DatabaseError | 8.4: HY000, 9.7: HY000 | A FULLTEXT parser plugin makes the statement unsafe for statement-based replication |
| 3007 | ER_CANNOT_DISCARD_TEMPORARY_TABLE | NotSupportedError | 8.4: HY000, 9.7: HY000 | cannot discard temporary table |
| 3008 | ER_FK_DEPTH_EXCEEDED | IntegrityError | 8.4: HY000, 9.7: HY000 | foreign key depth exceeded |
| 3009 | ER_COL_COUNT_DOESNT_MATCH_PLEASE_UPDATE_V2 | OperationalError | 8.4: HY000, 9.7: HY000 | A system table's column count differs from the required current definition |
| 3010 | ER_WARN_TRIGGER_DOESNT_HAVE_CREATED | DatabaseError | 8.4: HY000, 9.7: HY000 | The trigger definition lacks its creation timestamp |
| 3011 | ER_REFERENCED_TRG_DOES_NOT_EXIST | ProgrammingError | 8.4: HY000, 9.7: HY000 | The referenced trigger does not exist |
| 3012 | ER_EXPLAIN_NOT_SUPPORTED | NotSupportedError | 8.4: HY000, 9.7: HY000 | explain not supported |
| 3013 | ER_INVALID_FIELD_SIZE | ProgrammingError | 8.4: HY000, 9.7: HY000 | invalid field size |
| 3014 | ER_MISSING_HA_CREATE_OPTION | ProgrammingError | 8.4: HY000, 9.7: HY000 | A storage engine table-creation option is missing |
| 3015 | ER_ENGINE_OUT_OF_MEMORY | OperationalError | 8.4: HY000, 9.7: HY000 | The storage engine ran out of memory |
| 3016 | ER_PASSWORD_EXPIRE_ANONYMOUS_USER | ProgrammingError | 8.4: HY000, 9.7: HY000 | An anonymous account cannot use password expiration |
| 3017 | ER_REPLICA_SQL_THREAD_MUST_STOP | OperationalError | 8.4: HY000, 9.7: HY000 | replica SQL thread must stop |
| 3018 | ER_NO_FT_MATERIALIZED_SUBQUERY | NotSupportedError | 8.4: HY000, 9.7: HY000 | A materialized subquery cannot use this FULLTEXT operation |
| 3019 | ER_INNODB_UNDO_LOG_FULL | OperationalError | 8.4: HY000, 9.7: HY000 | innodb undo log full |
| 3020 | ER_INVALID_ARGUMENT_FOR_LOGARITHM | DataError | 8.4: 2201E, 9.7: 2201E | invalid argument for logarithm |
| 3021 | ER_REPLICA_CHANNEL_IO_THREAD_MUST_STOP | OperationalError | 8.4: HY000, 9.7: HY000 | replica channel I/O thread must stop |
| 3022 | ER_WARN_OPEN_TEMP_TABLES_MUST_BE_ZERO | DatabaseError | 8.4: HY000, 9.7: HY000 | Open replica temporary tables should be closed before the indicated replication operation |
| 3023 | ER_WARN_ONLY_SOURCE_LOG_FILE_NO_POS | DatabaseError | 8.4: HY000, 9.7: HY000 | A replication source log file was specified without a log position |
| 3024 | ER_QUERY_TIMEOUT | OperationalError | 8.4: HY000, 9.7: HY000 | The statement exceeded its maximum execution time |
| 3025 | ER_NON_RO_SELECT_DISABLE_TIMER | DatabaseError | 8.4: HY000, 9.7: HY000 | An execution timer is disabled for a SELECT that is not read-only |
| 3026 | ER_DUP_LIST_ENTRY | ProgrammingError | 8.4: HY000, 9.7: HY000 | A list contains a duplicate entry |
| 3028 | ER_AGGREGATE_ORDER_FOR_UNION | ProgrammingError | 8.4: HY000, 9.7: HY000 | ORDER BY in a combined query uses an invalid aggregate expression |
| 3029 | ER_AGGREGATE_ORDER_NON_AGG_QUERY | ProgrammingError | 8.4: HY000, 9.7: HY000 | ORDER BY uses an aggregate in a nonaggregate query |
| 3030 | ER_REPLICA_WORKER_STOPPED_PREVIOUS_THD_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | A replication worker stopped because another worker encountered an error |
| 3031 | ER_DONT_SUPPORT_REPLICA_PRESERVE_COMMIT_ORDER | NotSupportedError | 8.4: HY000 | The current configuration cannot preserve replica commit order |
| 3032 | ER_SERVER_OFFLINE_MODE | OperationalError | 8.4: HY000, 9.7: HY000 | server offline mode |
| 3033 | ER_GIS_DIFFERENT_SRIDS | DataError | 8.4: HY000, 9.7: HY000 | Geometry operands use different spatial reference identifiers |
| 3034 | ER_GIS_UNSUPPORTED_ARGUMENT | NotSupportedError | 8.4: HY000, 9.7: HY000 | A spatial function received an unsupported argument |
| 3035 | ER_GIS_UNKNOWN_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | A spatial operation failed without a more specific condition |
| 3036 | ER_GIS_UNKNOWN_EXCEPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | A spatial operation raised an unrecognized native exception |
| 3037 | ER_GIS_INVALID_DATA | DataError | 8.4: 22023, 9.7: 22023 | A geometry value is invalid |
| 3038 | ER_BOOST_GEOMETRY_EMPTY_INPUT_EXCEPTION | DataError | 8.4: HY000, 9.7: HY000 | A geometry operation cannot accept an empty input |
| 3039 | ER_BOOST_GEOMETRY_CENTROID_EXCEPTION | DataError | 8.4: HY000, 9.7: HY000 | Computing a geometry centroid failed |
| 3040 | ER_BOOST_GEOMETRY_OVERLAY_INVALID_INPUT_EXCEPTION | DataError | 8.4: HY000, 9.7: HY000 | A geometry overlay operation received invalid input |
| 3041 | ER_BOOST_GEOMETRY_TURN_INFO_EXCEPTION | DataError | 8.4: HY000, 9.7: HY000 | The geometry algorithm encountered invalid turn information |
| 3042 | ER_BOOST_GEOMETRY_SELF_INTERSECTION_POINT_EXCEPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | A geometry operation encountered an invalid self-intersection point |
| 3043 | ER_BOOST_GEOMETRY_UNKNOWN_EXCEPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | The geometry library raised an unrecognized exception |
| 3044 | ER_STD_BAD_ALLOC_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | A native standard-library allocation failed |
| 3045 | ER_STD_DOMAIN_ERROR | DataError | 8.4: HY000, 9.7: HY000 | A native standard-library operation received a value outside its mathematical domain |
| 3046 | ER_STD_LENGTH_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | A native standard-library operation exceeded a container length limit |
| 3047 | ER_STD_INVALID_ARGUMENT | DataError | 8.4: HY000, 9.7: HY000 | A native standard-library operation received an invalid argument |
| 3048 | ER_STD_OUT_OF_RANGE_ERROR | DataError | 8.4: HY000, 9.7: HY000 | A native standard-library access was outside the permitted range |
| 3049 | ER_STD_OVERFLOW_ERROR | DataError | 8.4: HY000, 9.7: HY000 | A native standard-library arithmetic operation overflowed |
| 3050 | ER_STD_RANGE_ERROR | DataError | 8.4: HY000, 9.7: HY000 | A native standard-library numeric operation reported a range error |
| 3051 | ER_STD_UNDERFLOW_ERROR | DataError | 8.4: HY000, 9.7: HY000 | A native standard-library arithmetic operation underflowed |
| 3052 | ER_STD_LOGIC_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | A native standard-library operation reported a logic error |
| 3053 | ER_STD_RUNTIME_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | A native standard-library operation reported a runtime error |
| 3054 | ER_STD_UNKNOWN_EXCEPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | An unrecognized native standard-library exception occurred |
| 3055 | ER_GIS_DATA_WRONG_ENDIANESS | DataError | 8.4: HY000, 9.7: HY000 | Geometry binary data has an invalid byte-order marker |
| 3056 | ER_CHANGE_SOURCE_PASSWORD_LENGTH | DataError | 8.4: HY000, 9.7: HY000 | The replication source password exceeds its supported length |
| 3057 | ER_USER_LOCK_WRONG_NAME | ProgrammingError | 8.4: 42000, 9.7: 42000 | The user-level lock name is invalid |
| 3058 | ER_USER_LOCK_DEADLOCK | OperationalError | 8.4: HY000, 9.7: HY000 | A deadlock occurred while acquiring a user-level lock |
| 3059 | ER_REPLACE_INACCESSIBLE_ROWS | ProgrammingError | 8.4: HY000, 9.7: HY000 | REPLACE would need to modify a row that is not accessible |
| 3060 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_GIS | NotSupportedError | 8.4: HY000, 9.7: HY000 | The requested ALTER TABLE algorithm does not support this spatial column change |
| 3061 | ER_ILLEGAL_USER_VAR | ProgrammingError | 8.4: 42000, 9.7: 42000 | illegal user variable |
| 3062 | ER_GTID_MODE_OFF | OperationalError | 8.4: HY000, 9.7: HY000 | Global transaction identifier mode is disabled |
| 3064 | ER_INCORRECT_TYPE | ProgrammingError | 8.4: HY000, 9.7: HY000 | incorrect type |
| 3065 | ER_FIELD_IN_ORDER_NOT_SELECT | ProgrammingError | 8.4: HY000, 9.7: HY000 | ORDER BY references an expression absent from a DISTINCT select list |
| 3066 | ER_AGGREGATE_IN_ORDER_NOT_SELECT | ProgrammingError | 8.4: HY000, 9.7: HY000 | ORDER BY uses an aggregate incompatible with the select list |
| 3067 | ER_INVALID_RPL_WILD_TABLE_FILTER_PATTERN | ProgrammingError | 8.4: HY000, 9.7: HY000 | A replication wildcard table-filter pattern is invalid |
| 3068 | ER_NET_OK_PACKET_TOO_LARGE | OperationalError | 8.4: 08S01, 9.7: 08S01 | The server's OK packet exceeds the permitted network packet size |
| 3069 | ER_INVALID_JSON_DATA | DataError | 8.4: HY000, 9.7: HY000 | invalid JSON data |
| 3070 | ER_INVALID_GEOJSON_MISSING_MEMBER | DataError | 8.4: HY000, 9.7: HY000 | A GeoJSON object lacks a required member |
| 3071 | ER_INVALID_GEOJSON_WRONG_TYPE | DataError | 8.4: HY000, 9.7: HY000 | A GeoJSON member has an invalid type |
| 3072 | ER_INVALID_GEOJSON_UNSPECIFIED | DataError | 8.4: HY000, 9.7: HY000 | The GeoJSON document violates a requirement described by the server message |
| 3073 | ER_DIMENSION_UNSUPPORTED | DataError | 8.4: HY000, 9.7: HY000 | The geometry's dimension is not supported |
| 3074 | ER_REPLICA_CHANNEL_DOES_NOT_EXIST | ProgrammingError | 8.4: HY000, 9.7: HY000 | replica channel does not exist |
| 3076 | ER_REPLICA_CHANNEL_NAME_INVALID_OR_TOO_LONG | ProgrammingError | 8.4: HY000, 9.7: HY000 | replica channel name invalid or too long |
| 3077 | ER_REPLICA_NEW_CHANNEL_WRONG_REPOSITORY | OperationalError | 8.4: HY000, 9.7: HY000 | Creating this replication channel requires a different metadata repository |
| 3079 | ER_REPLICA_MULTIPLE_CHANNELS_CMD | ProgrammingError | 8.4: HY000, 9.7: HY000 | The replication command is ambiguous because multiple channels exist |
| 3080 | ER_REPLICA_MAX_CHANNELS_EXCEEDED | OperationalError | 8.4: HY000, 9.7: HY000 | replica max channels exceeded |
| 3081 | ER_REPLICA_CHANNEL_MUST_STOP | OperationalError | 8.4: HY000, 9.7: HY000 | replica channel must stop |
| 3082 | ER_REPLICA_CHANNEL_NOT_RUNNING | OperationalError | 8.4: HY000, 9.7: HY000 | replica channel not running |
| 3083 | ER_REPLICA_CHANNEL_WAS_RUNNING | OperationalError | 8.4: HY000, 9.7: HY000 | replica channel was running |
| 3084 | ER_REPLICA_CHANNEL_WAS_NOT_RUNNING | OperationalError | 8.4: HY000, 9.7: HY000 | replica channel was not running |
| 3085 | ER_REPLICA_CHANNEL_SQL_THREAD_MUST_STOP | OperationalError | 8.4: HY000, 9.7: HY000 | replica channel SQL thread must stop |
| 3086 | ER_REPLICA_CHANNEL_SQL_SKIP_COUNTER | ProgrammingError | 8.4: HY000, 9.7: HY000 | The channel's SQL skip counter cannot be used in the current replication state |
| 3087 | ER_WRONG_FIELD_WITH_GROUP_V2 | ProgrammingError | 8.4: HY000, 9.7: HY000 | A selected expression violates the query's grouping requirements |
| 3088 | ER_MIX_OF_GROUP_FUNC_AND_FIELDS_V2 | ProgrammingError | 8.4: HY000, 9.7: HY000 | Nonaggregated expressions are mixed with aggregates without a valid grouping |
| 3089 | ER_WARN_DEPRECATED_SYSVAR_UPDATE | DatabaseError | 8.4: HY000, 9.7: HY000 | Updating the system variable uses a deprecated form |
| 3090 | ER_WARN_DEPRECATED_SQLMODE | DatabaseError | 8.4: HY000, 9.7: HY000 | The statement uses a deprecated SQL mode |
| 3091 | ER_CANNOT_LOG_PARTIAL_DROP_DATABASE_WITH_GTID | OperationalError | 8.4: HY000, 9.7: HY000 | A partially completed DROP DATABASE could not be fully recorded with a global transaction identifier |
| 3092 | ER_GROUP_REPLICATION_CONFIGURATION | OperationalError | 8.4: HY000, 9.7: HY000 | group replication configuration |
| 3093 | ER_GROUP_REPLICATION_RUNNING | OperationalError | 8.4: HY000, 9.7: HY000 | group replication running |
| 3094 | ER_GROUP_REPLICATION_APPLIER_INIT_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | The group replication applier could not be initialized |
| 3095 | ER_GROUP_REPLICATION_STOP_APPLIER_THREAD_TIMEOUT | OperationalError | 8.4: HY000, 9.7: HY000 | Stopping the group replication applier timed out |
| 3096 | ER_GROUP_REPLICATION_COMMUNICATION_LAYER_SESSION_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | The group communication session could not be established |
| 3097 | ER_GROUP_REPLICATION_COMMUNICATION_LAYER_JOIN_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | The member could not join group communication |
| 3098 | ER_BEFORE_DML_VALIDATION_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | Group replication rejected the transaction during pre-write validation |
| 3099 | ER_PREVENTS_VARIABLE_WITHOUT_RBR | ProgrammingError | 8.4: HY000, 9.7: HY000 | The variable requires row-based binary logging |
| 3100 | ER_RUN_HOOK_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | A server plugin hook failed |
| 3101 | ER_TRANSACTION_ROLLBACK_DURING_COMMIT | OperationalError | 8.4: 40000, 9.7: 40000 | transaction rollback during commit |
| 3102 | ER_GENERATED_COLUMN_FUNCTION_IS_NOT_ALLOWED | ProgrammingError | 8.4: HY000, 9.7: HY000 | generated column function is not allowed |
| 3103 | ER_UNSUPPORTED_ALTER_INPLACE_ON_VIRTUAL_COLUMN | NotSupportedError | 8.4: HY000, 9.7: HY000 | The requested in-place ALTER is unsupported for this virtual column |
| 3104 | ER_WRONG_FK_OPTION_FOR_GENERATED_COLUMN | ProgrammingError | 8.4: HY000, 9.7: HY000 | The foreign key action is invalid for the generated column |
| 3105 | ER_NON_DEFAULT_VALUE_FOR_GENERATED_COLUMN | ProgrammingError | 8.4: HY000, 9.7: HY000 | A generated column may receive only its DEFAULT value |
| 3106 | ER_UNSUPPORTED_ACTION_ON_GENERATED_COLUMN | NotSupportedError | 8.4: HY000, 9.7: HY000 | The requested operation is unsupported for the generated column |
| 3107 | ER_GENERATED_COLUMN_NON_PRIOR | ProgrammingError | 8.4: HY000, 9.7: HY000 | A generated column refers to a later-defined generated column |
| 3108 | ER_DEPENDENT_BY_GENERATED_COLUMN | ProgrammingError | 8.4: HY000, 9.7: HY000 | Another generated column depends on the column being modified |
| 3109 | ER_GENERATED_COLUMN_REF_AUTO_INC | ProgrammingError | 8.4: HY000, 9.7: HY000 | A generated column cannot reference an AUTO_INCREMENT column |
| 3110 | ER_FEATURE_NOT_AVAILABLE | NotSupportedError | 8.4: HY000, 9.7: HY000 | feature not available |
| 3111 | ER_CANT_SET_GTID_MODE | OperationalError | 8.4: HY000, 9.7: HY000 | The current server state prevents changing global transaction identifier mode |
| 3112 | ER_CANT_USE_AUTO_POSITION_WITH_GTID_MODE_OFF | OperationalError | 8.4: HY000, 9.7: HY000 | Replication auto-positioning requires global transaction identifier mode to be enabled |
| 3116 | ER_CANT_ENFORCE_GTID_CONSISTENCY_WITH_ONGOING_GTID_VIOLATING_TX | OperationalError | 8.4: HY000, 9.7: HY000 | Global transaction identifier consistency cannot be enforced while incompatible transactions remain |
| 3117 | ER_ENFORCE_GTID_CONSISTENCY_WARN_WITH_ONGOING_GTID_VIOLATING_TX | OperationalError | 8.4: HY000, 9.7: HY000 | Active transactions violate global transaction identifier consistency requirements |
| 3118 | ER_ACCOUNT_HAS_BEEN_LOCKED | OperationalError | 8.4: HY000, 9.7: HY000 | account has been locked |
| 3119 | ER_WRONG_TABLESPACE_NAME | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid tablespace name |
| 3120 | ER_TABLESPACE_IS_NOT_EMPTY | OperationalError | 8.4: HY000, 9.7: HY000 | tablespace is not empty |
| 3121 | ER_WRONG_FILE_NAME | ProgrammingError | 8.4: HY000, 9.7: HY000 | invalid file name |
| 3122 | ER_BOOST_GEOMETRY_INCONSISTENT_TURNS_EXCEPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | The geometry algorithm encountered inconsistent turn information |
| 3123 | ER_WARN_OPTIMIZER_HINT_SYNTAX_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | An optimizer hint has invalid syntax |
| 3124 | ER_WARN_BAD_MAX_EXECUTION_TIME | DatabaseError | 8.4: HY000, 9.7: HY000 | The MAX_EXECUTION_TIME hint is unsupported in the context described by the server message |
| 3125 | ER_WARN_UNSUPPORTED_MAX_EXECUTION_TIME | ProgrammingError | 8.4: HY000, 9.7: HY000 | A MAX_EXECUTION_TIME hint is unsupported in this query context |
| 3126 | ER_WARN_CONFLICTING_HINT | ProgrammingError | 8.4: HY000, 9.7: HY000 | Optimizer hints conflict with one another |
| 3127 | ER_WARN_UNKNOWN_QB_NAME | ProgrammingError | 8.4: HY000, 9.7: HY000 | An optimizer hint names an unknown query block |
| 3128 | ER_UNRESOLVED_HINT_NAME | ProgrammingError | 8.4: HY000, 9.7: HY000 | An optimizer hint refers to an unresolved table or index name |
| 3129 | ER_WARN_ON_MODIFYING_GTID_EXECUTED_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | Direct modification of the executed-transaction identifier table is discouraged |
| 3130 | ER_PLUGGABLE_PROTOCOL_COMMAND_NOT_SUPPORTED | NotSupportedError | 8.4: HY000, 9.7: HY000 | The selected connection protocol does not support this command |
| 3131 | ER_LOCKING_SERVICE_WRONG_NAME | ProgrammingError | 8.4: 42000, 9.7: 42000 | The locking-service lock name is invalid |
| 3132 | ER_LOCKING_SERVICE_DEADLOCK | OperationalError | 8.4: HY000, 9.7: HY000 | The locking service detected a deadlock |
| 3133 | ER_LOCKING_SERVICE_TIMEOUT | OperationalError | 8.4: HY000, 9.7: HY000 | A locking-service request timed out |
| 3134 | ER_GIS_MAX_POINTS_IN_GEOMETRY_OVERFLOWED | DataError | 8.4: HY000, 9.7: HY000 | A geometry contains more points than the supported limit |
| 3135 | ER_SQL_MODE_MERGED | DatabaseError | 8.4: HY000, 9.7: HY000 | The SQL mode combines component modes whose combined use is subject to the stated deprecation |
| 3136 | ER_VTOKEN_PLUGIN_TOKEN_MISMATCH | OperationalError | 8.4: HY000 | A version-token value does not match the registered value |
| 3137 | ER_VTOKEN_PLUGIN_TOKEN_NOT_FOUND | OperationalError | 8.4: HY000 | A required version token was not found |
| 3138 | ER_CANT_SET_VARIABLE_WHEN_OWNING_GTID | ProgrammingError | 8.4: HY000, 9.7: HY000 | A session that owns a transaction identifier cannot change this variable |
| 3139 | ER_REPLICA_CHANNEL_OPERATION_NOT_ALLOWED | NotSupportedError | 8.4: HY000, 9.7: HY000 | replica channel operation not allowed |
| 3140 | ER_INVALID_JSON_TEXT | DataError | 8.4: 22032, 9.7: 22032 | invalid JSON text |
| 3141 | ER_INVALID_JSON_TEXT_IN_PARAM | DataError | 8.4: 22032, 9.7: 22032 | A function argument contains invalid JSON text |
| 3142 | ER_INVALID_JSON_BINARY_DATA | DataError | 8.4: HY000, 9.7: HY000 | invalid JSON binary data |
| 3143 | ER_INVALID_JSON_PATH | DataError | 8.4: 42000, 9.7: 42000 | invalid JSON path |
| 3144 | ER_INVALID_JSON_CHARSET | DataError | 8.4: 22032, 9.7: 22032 | invalid JSON character set |
| 3145 | ER_INVALID_JSON_CHARSET_IN_FUNCTION | DataError | 8.4: 22032, 9.7: 22032 | invalid JSON character set in function |
| 3146 | ER_INVALID_TYPE_FOR_JSON | DataError | 8.4: 22032, 9.7: 22032 | invalid type for JSON |
| 3147 | ER_INVALID_CAST_TO_JSON | DataError | 8.4: 22032, 9.7: 22032 | invalid cast to JSON |
| 3148 | ER_INVALID_JSON_PATH_CHARSET | DataError | 8.4: 42000, 9.7: 42000 | invalid JSON path character set |
| 3149 | ER_INVALID_JSON_PATH_WILDCARD | DataError | 8.4: 42000, 9.7: 42000 | Wildcards are not allowed at this position in the JSON path |
| 3150 | ER_JSON_VALUE_TOO_BIG | DataError | 8.4: 22032, 9.7: 22032 | JSON value too big |
| 3151 | ER_JSON_KEY_TOO_BIG | DataError | 8.4: 22032, 9.7: 22032 | JSON key too big |
| 3152 | ER_JSON_USED_AS_KEY | ProgrammingError | 8.4: 42000, 9.7: 42000 | JSON used as key |
| 3153 | ER_JSON_VACUOUS_PATH | DataError | 8.4: 42000, 9.7: 42000 | The JSON path selects no usable location |
| 3154 | ER_JSON_BAD_ONE_OR_ALL_ARG | DataError | 8.4: 42000, 9.7: 42000 | The JSON search mode must be 'one' or 'all' |
| 3155 | ER_NUMERIC_JSON_VALUE_OUT_OF_RANGE | DataError | 8.4: 22003, 9.7: 22003 | numeric JSON value out of range |
| 3156 | ER_INVALID_JSON_VALUE_FOR_CAST | DataError | 8.4: 22018, 9.7: 22018 | invalid JSON value for cast |
| 3157 | ER_JSON_DOCUMENT_TOO_DEEP | DataError | 8.4: 22032, 9.7: 22032 | JSON document too deep |
| 3158 | ER_JSON_DOCUMENT_NULL_KEY | DataError | 8.4: 22032, 9.7: 22032 | A JSON object key must not be NULL |
| 3159 | ER_SECURE_TRANSPORT_REQUIRED | OperationalError | 8.4: HY000, 9.7: HY000 | secure transport required |
| 3160 | ER_NO_SECURE_TRANSPORTS_CONFIGURED | OperationalError | 8.4: HY000, 9.7: HY000 | no secure transports configured |
| 3161 | ER_DISABLED_STORAGE_ENGINE | NotSupportedError | 8.4: HY000, 9.7: HY000 | disabled storage engine |
| 3162 | ER_USER_DOES_NOT_EXIST | ProgrammingError | 8.4: HY000, 9.7: HY000 | user does not exist |
| 3163 | ER_USER_ALREADY_EXISTS | ProgrammingError | 8.4: HY000, 9.7: HY000 | user already exists |
| 3164 | ER_AUDIT_API_ABORT | OperationalError | 8.4: HY000, 9.7: HY000 | An audit plugin aborted the operation |
| 3165 | ER_INVALID_JSON_PATH_ARRAY_CELL | DataError | 8.4: 42000, 9.7: 42000 | The JSON path contains an invalid array index |
| 3166 | ER_BUFPOOL_RESIZE_INPROGRESS | OperationalError | 8.4: HY000, 9.7: HY000 | The InnoDB buffer pool is already being resized |
| 3167 | ER_FEATURE_DISABLED_SEE_DOC | NotSupportedError | 8.4: HY000, 9.7: HY000 | The requested feature is disabled; the server message identifies the relevant configuration |
| 3168 | ER_SERVER_ISNT_AVAILABLE | OperationalError | 8.4: HY000, 9.7: HY000 | The server is not available for normal client work |
| 3169 | ER_SESSION_WAS_KILLED | OperationalError | 8.4: HY000, 9.7: HY000 | session was killed |
| 3170 | ER_CAPACITY_EXCEEDED | OperationalError | 8.4: HY000, 9.7: HY000 | capacity exceeded |
| 3171 | ER_CAPACITY_EXCEEDED_IN_RANGE_OPTIMIZER | DatabaseError | 8.4: HY000, 9.7: HY000 | The range optimizer exceeded its memory limit |
| 3173 | ER_CANT_WAIT_FOR_EXECUTED_GTID_SET_WHILE_OWNING_A_GTID | ProgrammingError | 8.4: HY000, 9.7: HY000 | A session cannot wait for the transaction identifier it currently owns |
| 3174 | ER_CANNOT_ADD_FOREIGN_BASE_COL_VIRTUAL | NotSupportedError | 8.4: HY000, 9.7: HY000 | A foreign key cannot use the base column of this virtual generated column |
| 3175 | ER_CANNOT_CREATE_VIRTUAL_INDEX_CONSTRAINT | NotSupportedError | 8.4: HY000, 9.7: HY000 | An index constraint cannot be created on this virtual column |
| 3176 | ER_ERROR_ON_MODIFYING_GTID_EXECUTED_TABLE | ProgrammingError | 8.4: HY000, 9.7: HY000 | The executed-transaction identifier table cannot be modified inside this distributed transaction |
| 3177 | ER_LOCK_REFUSED_BY_ENGINE | OperationalError | 8.4: HY000, 9.7: HY000 | lock refused by engine |
| 3178 | ER_UNSUPPORTED_ALTER_ONLINE_ON_VIRTUAL_COLUMN | NotSupportedError | 8.4: HY000, 9.7: HY000 | unsupported alter online on virtual column |
| 3179 | ER_MASTER_KEY_ROTATION_NOT_SUPPORTED_BY_SE | NotSupportedError | 8.4: HY000, 9.7: HY000 | The storage engine does not support encryption-key rotation |
| 3181 | ER_MASTER_KEY_ROTATION_BINLOG_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | Recording encryption-key rotation in the binary log failed |
| 3182 | ER_MASTER_KEY_ROTATION_SE_UNAVAILABLE | OperationalError | 8.4: HY000, 9.7: HY000 | The storage engine needed for encryption-key rotation is unavailable |
| 3183 | ER_TABLESPACE_CANNOT_ENCRYPT | NotSupportedError | 8.4: HY000, 9.7: HY000 | tablespace cannot encrypt |
| 3184 | ER_INVALID_ENCRYPTION_OPTION | ProgrammingError | 8.4: HY000, 9.7: HY000 | invalid encryption option |
| 3185 | ER_CANNOT_FIND_KEY_IN_KEYRING | OperationalError | 8.4: HY000, 9.7: HY000 | cannot find key in keyring |
| 3186 | ER_CAPACITY_EXCEEDED_IN_PARSER | OperationalError | 8.4: HY000, 9.7: HY000 | capacity exceeded in parser |
| 3187 | ER_UNSUPPORTED_ALTER_ENCRYPTION_INPLACE | NotSupportedError | 8.4: HY000, 9.7: HY000 | The requested in-place ALTER cannot change tablespace encryption |
| 3188 | ER_KEYRING_UDF_KEYRING_SERVICE_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | A keyring function could not use the keyring service |
| 3189 | ER_USER_COLUMN_OLD_LENGTH | OperationalError | 8.4: HY000, 9.7: HY000 | A user-name column in a system table uses an obsolete length |
| 3190 | ER_CANT_RESET_SOURCE | OperationalError | 8.4: HY000, 9.7: HY000 | Resetting replication source state failed |
| 3191 | ER_GROUP_REPLICATION_MAX_GROUP_SIZE | OperationalError | 8.4: HY000, 9.7: HY000 | The replication group has reached its maximum member count |
| 3192 | ER_CANNOT_ADD_FOREIGN_BASE_COL_STORED | NotSupportedError | 8.4: HY000, 9.7: HY000 | A foreign key cannot use the base column of this stored generated column |
| 3193 | ER_TABLE_REFERENCED | OperationalError | 8.4: HY000, 9.7: HY000 | The table is referenced by a dependent object that prevents this operation |
| 3197 | ER_XA_RETRY | OperationalError | 8.4: HY000, 9.7: HY000 | The distributed transaction must be retried because of an incompatible state |
| 3198 | ER_KEYRING_AWS_UDF_AWS_KMS_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | A keyring function received an error from the cloud key-management service |
| 3199 | ER_BINLOG_UNSAFE_XA | DatabaseError | 8.4: HY000, 9.7: HY000 | The distributed transaction is unsafe for statement-based replication |
| 3200 | ER_UDF_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | A loadable function reported the failure described by the server message |
| 3201 | ER_KEYRING_MIGRATION_FAILURE | OperationalError | 8.4: HY000, 9.7: HY000 | keyring migration failure |
| 3202 | ER_KEYRING_ACCESS_DENIED_ERROR | OperationalError | 8.4: 42000, 9.7: 42000 | keyring access denied error |
| 3203 | ER_KEYRING_MIGRATION_STATUS | DatabaseError | 8.4: HY000, 9.7: HY000 | The server reports the keyring migration state |
| 3218 | ER_AUDIT_LOG_UDF_READ_INVALID_MAX_ARRAY_LENGTH_ARG_VALUE | DataError | 8.4: HY000, 9.7: HY000 | The audit-log reader's maximum array length argument is invalid |
| 3231 | ER_WRITE_SET_EXCEEDS_LIMIT | OperationalError | 8.4: HY000, 9.7: HY000 | write set exceeds limit |
| 3235 | ER_AES_INVALID_KDF_NAME | DataError | 8.4: HY000, 9.7: HY000 | The AES key-derivation function name is invalid |
| 3236 | ER_AES_INVALID_KDF_ITERATIONS | DataError | 8.4: HY000, 9.7: HY000 | The AES key-derivation iteration count is invalid |
| 3237 | WARN_AES_KEY_SIZE | DatabaseError | 8.4: HY000, 9.7: HY000 | The AES key or key-derivation settings have the strength limitation described by the server message |
| 3238 | ER_AES_INVALID_KDF_OPTION_SIZE | DataError | 8.4: HY000, 9.7: HY000 | An AES key-derivation option exceeds its supported size |
| 3500 | ER_UNSUPPORT_COMPRESSED_TEMPORARY_TABLE | NotSupportedError | 8.4: HY000, 9.7: HY000 | Compressed temporary tables are not supported |
| 3501 | ER_ACL_OPERATION_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | An account or privilege-management operation failed |
| 3502 | ER_UNSUPPORTED_INDEX_ALGORITHM | NotSupportedError | 8.4: HY000, 9.7: HY000 | unsupported index algorithm |
| 3503 | ER_NO_SUCH_DB | ProgrammingError | 8.4: 42Y07, 9.7: 42Y07 | no such database |
| 3504 | ER_TOO_BIG_ENUM | ProgrammingError | 8.4: HY000, 9.7: HY000 | An ENUM definition has too many members |
| 3505 | ER_TOO_LONG_SET_ENUM_VALUE | ProgrammingError | 8.4: HY000, 9.7: HY000 | An ENUM or SET member exceeds its permitted length |
| 3506 | ER_INVALID_DD_OBJECT | InternalError | 8.4: HY000, 9.7: HY000 | A data dictionary object is invalid |
| 3507 | ER_UPDATING_DD_TABLE | OperationalError | 8.4: HY000, 9.7: HY000 | Updating a data dictionary table failed |
| 3508 | ER_INVALID_DD_OBJECT_ID | InternalError | 8.4: HY000, 9.7: HY000 | A data dictionary object identifier is invalid |
| 3509 | ER_INVALID_DD_OBJECT_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | A data dictionary object name is invalid |
| 3510 | ER_TABLESPACE_MISSING_WITH_NAME | ProgrammingError | 8.4: HY000, 9.7: HY000 | tablespace missing with name |
| 3511 | ER_TOO_LONG_ROUTINE_COMMENT | ProgrammingError | 8.4: HY000, 9.7: HY000 | too long routine comment |
| 3512 | ER_SP_LOAD_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | The stored procedure or function could not be loaded |
| 3513 | ER_INVALID_BITWISE_OPERANDS_SIZE | DataError | 8.4: HY000, 9.7: HY000 | invalid bitwise operands size |
| 3514 | ER_INVALID_BITWISE_AGGREGATE_OPERANDS_SIZE | DataError | 8.4: HY000, 9.7: HY000 | invalid bitwise aggregate operands size |
| 3515 | ER_WARN_UNSUPPORTED_HINT | NotSupportedError | 8.4: HY000, 9.7: HY000 | The optimizer hint is not supported |
| 3516 | ER_UNEXPECTED_GEOMETRY_TYPE | DataError | 8.4: 22S01, 9.7: 22S01 | unexpected geometry type |
| 3517 | ER_SRS_PARSE_ERROR | DataError | 8.4: SR002, 9.7: SR002 | spatial reference system parse error |
| 3518 | ER_SRS_PROJ_PARAMETER_MISSING | DataError | 8.4: SR003, 9.7: SR003 | The spatial reference system lacks a required projection parameter |
| 3519 | ER_WARN_SRS_NOT_FOUND | ProgrammingError | 8.4: 01000, 9.7: 01000 | warn spatial reference system not found |
| 3520 | ER_SRS_NOT_CARTESIAN | DataError | 8.4: 22S00, 9.7: 22S00 | spatial reference system not cartesian |
| 3521 | ER_SRS_NOT_CARTESIAN_UNDEFINED | ProgrammingError | 8.4: SR001, 9.7: SR001 | The operation requires a defined Cartesian spatial reference system |
| 3522 | ER_PK_INDEX_CANT_BE_INVISIBLE | ProgrammingError | 8.4: HY000, 9.7: HY000 | primary key index cannot be invisible |
| 3523 | ER_UNKNOWN_AUTHID | ProgrammingError | 8.4: HY000, 9.7: HY000 | The specified authorization identifier is unknown |
| 3524 | ER_FAILED_ROLE_GRANT | OperationalError | 8.4: HY000, 9.7: HY000 | Granting a role failed |
| 3525 | ER_OPEN_ROLE_TABLES | OperationalError | 8.4: HY000, 9.7: HY000 | The role metadata tables could not be opened |
| 3526 | ER_FAILED_DEFAULT_ROLES | OperationalError | 8.4: HY000, 9.7: HY000 | Setting an account's default roles failed |
| 3527 | ER_COMPONENTS_NO_SCHEME | ProgrammingError | 8.4: HY000, 9.7: HY000 | A component identifier has no URI scheme |
| 3528 | ER_COMPONENTS_NO_SCHEME_SERVICE | OperationalError | 8.4: HY000, 9.7: HY000 | The component scheme has no registered loader service |
| 3529 | ER_COMPONENTS_CANT_LOAD | OperationalError | 8.4: HY000, 9.7: HY000 | components cannot load |
| 3530 | ER_ROLE_NOT_GRANTED | OperationalError | 8.4: HY000, 9.7: HY000 | role not granted |
| 3531 | ER_FAILED_REVOKE_ROLE | OperationalError | 8.4: HY000, 9.7: HY000 | failed revoke role |
| 3532 | ER_RENAME_ROLE | ProgrammingError | 8.4: HY000, 9.7: HY000 | A role account cannot be renamed in this way |
| 3533 | ER_COMPONENTS_CANT_ACQUIRE_SERVICE_IMPLEMENTATION | OperationalError | 8.4: HY000, 9.7: HY000 | A component could not acquire a required service implementation |
| 3534 | ER_COMPONENTS_CANT_SATISFY_DEPENDENCY | OperationalError | 8.4: HY000, 9.7: HY000 | A component dependency could not be satisfied |
| 3535 | ER_COMPONENTS_LOAD_CANT_REGISTER_SERVICE_IMPLEMENTATION | OperationalError | 8.4: HY000, 9.7: HY000 | A component's service implementation could not be registered |
| 3536 | ER_COMPONENTS_LOAD_CANT_INITIALIZE | OperationalError | 8.4: HY000, 9.7: HY000 | A component could not be initialized |
| 3537 | ER_COMPONENTS_UNLOAD_NOT_LOADED | ProgrammingError | 8.4: HY000, 9.7: HY000 | The component to be unloaded is not loaded |
| 3538 | ER_COMPONENTS_UNLOAD_CANT_DEINITIALIZE | OperationalError | 8.4: HY000, 9.7: HY000 | A component could not be deinitialized |
| 3539 | ER_COMPONENTS_CANT_RELEASE_SERVICE | OperationalError | 8.4: HY000, 9.7: HY000 | A component could not release a service |
| 3540 | ER_COMPONENTS_UNLOAD_CANT_UNREGISTER_SERVICE | OperationalError | 8.4: HY000, 9.7: HY000 | A component's service implementation could not be unregistered |
| 3541 | ER_COMPONENTS_CANT_UNLOAD | OperationalError | 8.4: HY000, 9.7: HY000 | A component could not be unloaded |
| 3542 | ER_WARN_UNLOAD_THE_NOT_PERSISTED | DatabaseError | 8.4: HY000, 9.7: HY000 | The component has no persisted INSTALL COMPONENT registration |
| 3543 | ER_COMPONENT_TABLE_INCORRECT | OperationalError | 8.4: HY000, 9.7: HY000 | The component registration table has an invalid definition |
| 3544 | ER_COMPONENT_MANIPULATE_ROW_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | A component registration row could not be changed |
| 3545 | ER_COMPONENTS_UNLOAD_DUPLICATE_IN_GROUP | ProgrammingError | 8.4: HY000, 9.7: HY000 | The unload group contains the same component more than once |
| 3546 | ER_CANT_SET_GTID_PURGED_DUE_SETS_CONSTRAINTS | ProgrammingError | 8.4: HY000, 9.7: HY000 | The new GTID_PURGED value violates the required transaction-identifier set relationships |
| 3547 | ER_CANNOT_LOCK_USER_MANAGEMENT_CACHES | OperationalError | 8.4: HY000, 9.7: HY000 | cannot lock user management caches |
| 3548 | ER_SRS_NOT_FOUND | ProgrammingError | 8.4: SR001, 9.7: SR001 | spatial reference system not found |
| 3549 | ER_VARIABLE_NOT_PERSISTED | OperationalError | 8.4: HY000, 9.7: HY000 | variable not persisted |
| 3550 | ER_IS_QUERY_INVALID_CLAUSE | ProgrammingError | 8.4: HY000, 9.7: HY000 | The INFORMATION_SCHEMA query contains an invalid clause |
| 3551 | ER_UNABLE_TO_STORE_STATISTICS | OperationalError | 8.4: HY000, 9.7: HY000 | unable to store statistics |
| 3552 | ER_NO_SYSTEM_SCHEMA_ACCESS | OperationalError | 8.4: HY000, 9.7: HY000 | no system schema access |
| 3553 | ER_NO_SYSTEM_TABLESPACE_ACCESS | OperationalError | 8.4: HY000, 9.7: HY000 | no system tablespace access |
| 3554 | ER_NO_SYSTEM_TABLE_ACCESS | OperationalError | 8.4: HY000, 9.7: HY000 | no system table access |
| 3555 | ER_NO_SYSTEM_TABLE_ACCESS_FOR_DICTIONARY_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | The server message identifies a data dictionary table category without an independent failure condition |
| 3556 | ER_NO_SYSTEM_TABLE_ACCESS_FOR_SYSTEM_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | The server message identifies a system table category without an independent failure condition |
| 3557 | ER_NO_SYSTEM_TABLE_ACCESS_FOR_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | The server message identifies a table category without an independent failure condition |
| 3558 | ER_INVALID_OPTION_KEY | DataError | 8.4: 22023, 9.7: 22023 | invalid option key |
| 3559 | ER_INVALID_OPTION_VALUE | DataError | 8.4: 22023, 9.7: 22023 | invalid option value |
| 3560 | ER_INVALID_OPTION_KEY_VALUE_PAIR | DataError | 8.4: 22023, 9.7: 22023 | invalid option key value pair |
| 3561 | ER_INVALID_OPTION_START_CHARACTER | DataError | 8.4: 22023, 9.7: 22023 | invalid option start character |
| 3562 | ER_INVALID_OPTION_END_CHARACTER | DataError | 8.4: 22023, 9.7: 22023 | invalid option end character |
| 3563 | ER_INVALID_OPTION_CHARACTERS | DataError | 8.4: 22023, 9.7: 22023 | invalid option characters |
| 3564 | ER_DUPLICATE_OPTION_KEY | DataError | 8.4: 22023, 9.7: 22023 | duplicate option key |
| 3565 | ER_WARN_SRS_NOT_FOUND_AXIS_ORDER | DatabaseError | 8.4: 01000, 9.7: 01000 | The spatial reference system is unknown; axis-order handling cannot be determined |
| 3566 | ER_NO_ACCESS_TO_NATIVE_FCT | OperationalError | 8.4: HY000, 9.7: HY000 | The account is not permitted to invoke the built-in function |
| 3567 | ER_RESET_SOURCE_TO_VALUE_OUT_OF_RANGE | DataError | 8.4: HY000, 9.7: HY000 | The replication source reset target is outside its permitted range |
| 3568 | ER_UNRESOLVED_TABLE_LOCK | ProgrammingError | 8.4: HY000, 9.7: HY000 | The requested table lock could not be resolved |
| 3569 | ER_DUPLICATE_TABLE_LOCK | ProgrammingError | 8.4: HY000, 9.7: HY000 | A table lock is specified more than once |
| 3570 | ER_BINLOG_UNSAFE_SKIP_LOCKED | DatabaseError | 8.4: HY000, 9.7: HY000 | SKIP LOCKED is unsafe for statement-based replication |
| 3571 | ER_BINLOG_UNSAFE_NOWAIT | DatabaseError | 8.4: HY000, 9.7: HY000 | NOWAIT is unsafe for statement-based replication |
| 3572 | ER_LOCK_NOWAIT | OperationalError | 8.4: HY000, 9.7: HY000 | A lock was unavailable and NOWAIT prohibited waiting |
| 3573 | ER_CTE_RECURSIVE_REQUIRES_UNION | ProgrammingError | 8.4: HY000, 9.7: HY000 | A recursive common table expression requires UNION |
| 3574 | ER_CTE_RECURSIVE_REQUIRES_NONRECURSIVE_FIRST | ProgrammingError | 8.4: HY000, 9.7: HY000 | A recursive common table expression must place its nonrecursive part first |
| 3575 | ER_CTE_RECURSIVE_FORBIDS_AGGREGATION | ProgrammingError | 8.4: HY000, 9.7: HY000 | The recursive part of a common table expression cannot use the specified aggregation |
| 3576 | ER_CTE_RECURSIVE_FORBIDDEN_JOIN_ORDER | ProgrammingError | 8.4: HY000, 9.7: HY000 | A recursive common table expression uses a prohibited join order |
| 3577 | ER_CTE_RECURSIVE_REQUIRES_SINGLE_REFERENCE | ProgrammingError | 8.4: HY000, 9.7: HY000 | The recursive part must reference the common table expression exactly once |
| 3578 | ER_SWITCH_TMP_ENGINE | OperationalError | 8.4: HY000, 9.7: HY000 | The server switched the storage engine used for an internal temporary table |
| 3579 | ER_WINDOW_NO_SUCH_WINDOW | ProgrammingError | 8.4: HY000, 9.7: HY000 | The named window is not defined |
| 3580 | ER_WINDOW_CIRCULARITY_IN_WINDOW_GRAPH | ProgrammingError | 8.4: HY000, 9.7: HY000 | The window definitions contain a circular reference |
| 3581 | ER_WINDOW_NO_CHILD_PARTITIONING | ProgrammingError | 8.4: HY000, 9.7: HY000 | An inherited window cannot add PARTITION BY |
| 3582 | ER_WINDOW_NO_INHERIT_FRAME | ProgrammingError | 8.4: HY000, 9.7: HY000 | A window with an explicit frame cannot be inherited |
| 3583 | ER_WINDOW_NO_REDEFINE_ORDER_BY | ProgrammingError | 8.4: HY000, 9.7: HY000 | An inherited window cannot redefine ORDER BY |
| 3584 | ER_WINDOW_FRAME_START_ILLEGAL | ProgrammingError | 8.4: HY000, 9.7: HY000 | The window frame's starting bound is invalid |
| 3585 | ER_WINDOW_FRAME_END_ILLEGAL | ProgrammingError | 8.4: HY000, 9.7: HY000 | The window frame's ending bound is invalid |
| 3586 | ER_WINDOW_FRAME_ILLEGAL | ProgrammingError | 8.4: HY000, 9.7: HY000 | The window frame's bounds are incompatible |
| 3587 | ER_WINDOW_RANGE_FRAME_ORDER_TYPE | ProgrammingError | 8.4: HY000, 9.7: HY000 | A RANGE frame requires a compatible ORDER BY type |
| 3588 | ER_WINDOW_RANGE_FRAME_TEMPORAL_TYPE | ProgrammingError | 8.4: HY000, 9.7: HY000 | A temporal RANGE frame requires a compatible temporal ordering expression |
| 3589 | ER_WINDOW_RANGE_FRAME_NUMERIC_TYPE | ProgrammingError | 8.4: HY000, 9.7: HY000 | A numeric RANGE frame requires a compatible numeric ordering expression |
| 3590 | ER_WINDOW_RANGE_BOUND_NOT_CONSTANT | ProgrammingError | 8.4: HY000, 9.7: HY000 | The window RANGE bound must be constant |
| 3591 | ER_WINDOW_DUPLICATE_NAME | ProgrammingError | 8.4: HY000, 9.7: HY000 | A window name is defined more than once |
| 3592 | ER_WINDOW_ILLEGAL_ORDER_BY | ProgrammingError | 8.4: HY000, 9.7: HY000 | ORDER BY is not valid for the specified window function |
| 3593 | ER_WINDOW_INVALID_WINDOW_FUNC_USE | ProgrammingError | 8.4: HY000, 9.7: HY000 | A window function is used in a prohibited query context |
| 3594 | ER_WINDOW_INVALID_WINDOW_FUNC_ALIAS_USE | ProgrammingError | 8.4: HY000, 9.7: HY000 | A window function alias is referenced in a prohibited context |
| 3595 | ER_WINDOW_NESTED_WINDOW_FUNC_USE_IN_WINDOW_SPEC | ProgrammingError | 8.4: HY000, 9.7: HY000 | A window specification cannot contain a nested window function |
| 3596 | ER_WINDOW_ROWS_INTERVAL_USE | ProgrammingError | 8.4: HY000, 9.7: HY000 | An INTERVAL bound is not allowed in a ROWS frame |
| 3597 | ER_WINDOW_NO_GROUP_ORDER_UNUSED | ProgrammingError | 8.4: HY000, 9.7: HY000 | The query uses an unsupported grouping or ordering form for a window |
| 3598 | ER_WINDOW_EXPLAIN_JSON | ProgrammingError | 8.4: HY000, 9.7: HY000 | This window query cannot be explained in the requested JSON format |
| 3599 | ER_WINDOW_FUNCTION_IGNORES_FRAME | DatabaseError | 8.4: HY000, 9.7: HY000 | The window function ignores the supplied frame clause |
| 3600 | ER_WL9236_NOW_UNUSED | OperationalError | 8.4: HY000, 9.7: HY000 | Window processing requires the TempTable internal temporary-memory storage engine |
| 3601 | ER_INVALID_NO_OF_ARGS | ProgrammingError | 8.4: HY000, 9.7: HY000 | The function was called with the wrong number of arguments |
| 3602 | ER_FIELD_IN_GROUPING_NOT_GROUP_BY | ProgrammingError | 8.4: HY000, 9.7: HY000 | GROUPING references an expression absent from GROUP BY |
| 3603 | ER_TOO_LONG_TABLESPACE_COMMENT | ProgrammingError | 8.4: HY000, 9.7: HY000 | too long tablespace comment |
| 3604 | ER_ENGINE_CANT_DROP_TABLE | OperationalError | 8.4: HY000, 9.7: HY000 | engine cannot drop table |
| 3605 | ER_ENGINE_CANT_DROP_MISSING_TABLE | OperationalError | 8.4: HY000, 9.7: HY000 | engine cannot drop missing table |
| 3606 | ER_TABLESPACE_DUP_FILENAME | ProgrammingError | 8.4: HY000, 9.7: HY000 | The tablespace definition repeats a file name |
| 3607 | ER_DB_DROP_RMDIR2 | OperationalError | 8.4: HY000, 9.7: HY000 | Dropping a database failed while removing its directory |
| 3608 | ER_IMP_NO_FILES_MATCHED | ProgrammingError | 8.4: HY000, 9.7: HY000 | No files match the import request |
| 3609 | ER_IMP_SCHEMA_DOES_NOT_EXIST | ProgrammingError | 8.4: HY000, 9.7: HY000 | The schema required by the import does not exist |
| 3610 | ER_IMP_TABLE_ALREADY_EXISTS | ProgrammingError | 8.4: HY000, 9.7: HY000 | The table to be imported already exists |
| 3611 | ER_IMP_INCOMPATIBLE_MYSQLD_VERSION | NotSupportedError | 8.4: HY000, 9.7: HY000 | The imported metadata comes from an incompatible MySQL server version |
| 3612 | ER_IMP_INCOMPATIBLE_DD_VERSION | NotSupportedError | 8.4: HY000, 9.7: HY000 | The imported metadata has an incompatible data dictionary version |
| 3613 | ER_IMP_INCOMPATIBLE_SDI_VERSION | NotSupportedError | 8.4: HY000, 9.7: HY000 | The imported metadata has an incompatible serialized dictionary version |
| 3614 | ER_WARN_INVALID_HINT | ProgrammingError | 8.4: HY000, 9.7: HY000 | The optimizer hint is invalid |
| 3615 | ER_VAR_DOES_NOT_EXIST | ProgrammingError | 8.4: HY000, 9.7: HY000 | variable does not exist |
| 3616 | ER_LONGITUDE_OUT_OF_RANGE | DataError | 8.4: 22S02, 9.7: 22S02 | longitude out of range |
| 3617 | ER_LATITUDE_OUT_OF_RANGE | DataError | 8.4: 22S03, 9.7: 22S03 | latitude out of range |
| 3618 | ER_NOT_IMPLEMENTED_FOR_GEOGRAPHIC_SRS | NotSupportedError | 8.4: 22S00, 9.7: 22S00 | not implemented for geographic spatial reference system |
| 3619 | ER_ILLEGAL_PRIVILEGE_LEVEL | ProgrammingError | 8.4: HY000, 9.7: HY000 | illegal privilege level |
| 3620 | ER_NO_SYSTEM_VIEW_ACCESS | OperationalError | 8.4: HY000, 9.7: HY000 | no system view access |
| 3621 | ER_COMPONENT_FILTER_FLABBERGASTED | DatabaseError | 8.4: HY000, 9.7: HY000 | The error-log filter component reports a diagnostic without a more specific fixed cause |
| 3622 | ER_PART_EXPR_TOO_LONG | ProgrammingError | 8.4: HY000, 9.7: HY000 | The partitioning expression exceeds its maximum length |
| 3623 | ER_UDF_DROP_DYNAMICALLY_REGISTERED | ProgrammingError | 8.4: HY000, 9.7: HY000 | A dynamically registered loadable function cannot be dropped using this command |
| 3624 | ER_UNABLE_TO_STORE_COLUMN_STATISTICS | OperationalError | 8.4: HY000, 9.7: HY000 | unable to store column statistics |
| 3625 | ER_UNABLE_TO_UPDATE_COLUMN_STATISTICS | OperationalError | 8.4: HY000, 9.7: HY000 | unable to update column statistics |
| 3626 | ER_UNABLE_TO_DROP_COLUMN_STATISTICS | OperationalError | 8.4: HY000, 9.7: HY000 | unable to drop column statistics |
| 3627 | ER_UNABLE_TO_BUILD_HISTOGRAM | OperationalError | 8.4: HY000, 9.7: HY000 | unable to build histogram |
| 3628 | ER_MANDATORY_ROLE | OperationalError | 8.4: HY000, 9.7: HY000 | The operation is prohibited for a mandatory role |
| 3629 | ER_MISSING_TABLESPACE_FILE | ProgrammingError | 8.4: HY000, 9.7: HY000 | missing tablespace file |
| 3630 | ER_PERSIST_ONLY_ACCESS_DENIED_ERROR | OperationalError | 8.4: 42000, 9.7: 42000 | The account lacks permission to persist the system variable |
| 3631 | ER_CMD_NEED_SUPER | OperationalError | 8.4: HY000 | The command requires an additional administrative privilege |
| 3632 | ER_PATH_IN_DATADIR | ProgrammingError | 8.4: HY000, 9.7: HY000 | The configured path is inside the server data directory |
| 3633 | ER_CLONE_DDL_IN_PROGRESS | OperationalError | 8.4: HY000, 9.7: HY000 | A concurrent schema-changing operation prevents cloning |
| 3634 | ER_CLONE_TOO_MANY_CONCURRENT_CLONES | OperationalError | 8.4: HY000, 9.7: HY000 | clone too many concurrent clones |
| 3635 | ER_APPLIER_LOG_EVENT_VALIDATION_ERROR | ProgrammingError | 8.4: HY000, 9.7: HY000 | The replication applier rejected a log event during validation |
| 3636 | ER_CTE_MAX_RECURSION_DEPTH | OperationalError | 8.4: HY000, 9.7: HY000 | The recursive common table expression exceeded its recursion limit |
| 3637 | ER_NOT_HINT_UPDATABLE_VARIABLE | ProgrammingError | 8.4: HY000, 9.7: HY000 | The variable cannot be changed through a SET_VAR optimizer hint |
| 3638 | ER_CREDENTIALS_CONTRADICT_TO_HISTORY | DataError | 8.4: HY000, 9.7: HY000 | The new credentials violate the password reuse history |
| 3639 | ER_WARNING_PASSWORD_HISTORY_CLAUSES_VOID | NotSupportedError | 8.4: HY000, 9.7: HY000 | Password history options have no effect in this context |
| 3640 | ER_CLIENT_DOES_NOT_SUPPORT | NotSupportedError | 8.4: HY000, 9.7: HY000 | client does not support |
| 3641 | ER_I_S_SKIPPED_TABLESPACE | OperationalError | 8.4: HY000, 9.7: HY000 | An INFORMATION_SCHEMA scan skipped the inaccessible tablespace |
| 3642 | ER_TABLESPACE_ENGINE_MISMATCH | ProgrammingError | 8.4: HY000, 9.7: HY000 | tablespace engine mismatch |
| 3643 | ER_WRONG_SRID_FOR_COLUMN | DataError | 8.4: HY000, 9.7: HY000 | The spatial reference identifier is invalid for the column |
| 3644 | ER_CANNOT_ALTER_SRID_DUE_TO_INDEX | ProgrammingError | 8.4: HY000, 9.7: HY000 | The spatial reference identifier cannot be changed while the spatial index exists |
| 3645 | ER_WARN_BINLOG_PARTIAL_UPDATES_DISABLED | DatabaseError | 8.4: HY000, 9.7: HY000 | Partial JSON binary-log updates are disabled in the current configuration |
| 3647 | ER_WARN_BINLOG_PARTIAL_UPDATES_SUGGESTS_PARTIAL_IMAGES | DatabaseError | 8.4: HY000, 9.7: HY000 | Partial JSON binary-log updates should be combined with partial row images |
| 3648 | ER_COULD_NOT_APPLY_JSON_DIFF | OperationalError | 8.4: HY000, 9.7: HY000 | could not apply JSON diff |
| 3649 | ER_CORRUPTED_JSON_DIFF | InternalError | 8.4: HY000, 9.7: HY000 | corrupted JSON diff |
| 3650 | ER_RESOURCE_GROUP_EXISTS | ProgrammingError | 8.4: HY000, 9.7: HY000 | resource group exists |
| 3651 | ER_RESOURCE_GROUP_NOT_EXISTS | ProgrammingError | 8.4: HY000, 9.7: HY000 | resource group not exists |
| 3652 | ER_INVALID_VCPU_ID | DataError | 8.4: HY000, 9.7: HY000 | The virtual CPU identifier is invalid |
| 3653 | ER_INVALID_VCPU_RANGE | DataError | 8.4: HY000, 9.7: HY000 | The virtual CPU range is invalid |
| 3654 | ER_INVALID_THREAD_PRIORITY | DataError | 8.4: HY000, 9.7: HY000 | invalid thread priority |
| 3655 | ER_DISALLOWED_OPERATION | DatabaseError | 8.4: HY000, 9.7: HY000 | The operation is not permitted in the current resource-group context |
| 3656 | ER_RESOURCE_GROUP_BUSY | OperationalError | 8.4: HY000, 9.7: HY000 | resource group busy |
| 3657 | ER_RESOURCE_GROUP_DISABLED | OperationalError | 8.4: HY000, 9.7: HY000 | resource group disabled |
| 3658 | ER_FEATURE_UNSUPPORTED | NotSupportedError | 8.4: HY000, 9.7: HY000 | feature unsupported |
| 3659 | ER_ATTRIBUTE_IGNORED | DatabaseError | 8.4: HY000, 9.7: HY000 | attribute ignored |
| 3660 | ER_INVALID_THREAD_ID | ProgrammingError | 8.4: HY000, 9.7: HY000 | invalid thread id |
| 3661 | ER_RESOURCE_GROUP_BIND_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | The thread could not be bound to the resource group |
| 3662 | ER_INVALID_USE_OF_FORCE_OPTION | ProgrammingError | 8.4: HY000, 9.7: HY000 | invalid use of force option |
| 3663 | ER_GROUP_REPLICATION_COMMAND_FAILURE | OperationalError | 8.4: HY000, 9.7: HY000 | group replication command failure |
| 3664 | ER_SDI_OPERATION_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | An operation on serialized dictionary information failed |
| 3665 | ER_MISSING_JSON_TABLE_VALUE | DataError | 8.4: 22035, 9.7: 22035 | A required JSON_TABLE value is missing |
| 3666 | ER_WRONG_JSON_TABLE_VALUE | DataError | 8.4: 2203F, 9.7: 2203F | A JSON_TABLE value is invalid for its target column |
| 3667 | ER_TF_MUST_HAVE_ALIAS | ProgrammingError | 8.4: 42000, 9.7: 42000 | A table function requires an alias |
| 3668 | ER_TF_FORBIDDEN_JOIN_TYPE | ProgrammingError | 8.4: HY000, 9.7: HY000 | The table function is used with a prohibited join type |
| 3669 | ER_JT_VALUE_OUT_OF_RANGE | DataError | 8.4: 22003, 9.7: 22003 | A JSON_TABLE value is outside its target column's range |
| 3670 | ER_JT_MAX_NESTED_PATH | ProgrammingError | 8.4: 42000, 9.7: 42000 | JSON_TABLE exceeds the maximum nested-path depth |
| 3671 | ER_PASSWORD_EXPIRATION_NOT_SUPPORTED_BY_AUTH_METHOD | NotSupportedError | 8.4: HY000, 9.7: HY000 | The authentication method does not support password expiration |
| 3672 | ER_INVALID_GEOJSON_CRS_NOT_TOP_LEVEL | DataError | 8.4: HY000, 9.7: HY000 | A GeoJSON coordinate reference system must be specified at the top level |
| 3673 | ER_BAD_NULL_ERROR_NOT_IGNORED | IntegrityError | 8.4: 23000, 9.7: 23000 | A NULL value violates a nonnullable column and cannot be ignored |
| 3674 | WARN_USELESS_SPATIAL_INDEX | DatabaseError | 8.4: HY000, 9.7: HY000 | The spatial index cannot be used effectively without the required spatial reference restriction |
| 3675 | ER_DISK_FULL_NOWAIT | OperationalError | 8.4: HY000, 9.7: HY000 | The disk is full and the operation is not configured to wait for space |
| 3676 | ER_PARSE_ERROR_IN_DIGEST_FN | DataError | 8.4: HY000, 9.7: HY000 | The SQL digest function received invalid statement syntax |
| 3677 | ER_UNDISCLOSED_PARSE_ERROR_IN_DIGEST_FN | DataError | 8.4: HY000, 9.7: HY000 | The SQL digest function could not parse the statement |
| 3678 | ER_SCHEMA_DIR_EXISTS | OperationalError | 8.4: HY000, 9.7: HY000 | A leftover schema directory prevents the requested schema operation |
| 3679 | ER_SCHEMA_DIR_MISSING | OperationalError | 8.4: HY000, 9.7: HY000 | The schema's filesystem directory is missing |
| 3680 | ER_SCHEMA_DIR_CREATE_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | The schema directory could not be created |
| 3681 | ER_SCHEMA_DIR_UNKNOWN | OperationalError | 8.4: HY000, 9.7: HY000 | The schema directory has an unexpected filesystem state |
| 3682 | ER_ONLY_IMPLEMENTED_FOR_SRID_0_AND_4326 | NotSupportedError | 8.4: 22S00, 9.7: 22S00 | The operation supports only spatial reference identifiers 0 and 4326 |
| 3684 | ER_REGEXP_BUFFER_OVERFLOW | DataError | 8.4: HY000, 9.7: HY000 | regexp buffer overflow |
| 3685 | ER_REGEXP_ILLEGAL_ARGUMENT | DataError | 8.4: HY000, 9.7: HY000 | regexp illegal argument |
| 3686 | ER_REGEXP_INDEX_OUTOFBOUNDS_ERROR | DataError | 8.4: HY000, 9.7: HY000 | A regular-expression index is outside its permitted bounds |
| 3687 | ER_REGEXP_INTERNAL_ERROR | InternalError | 8.4: HY000, 9.7: HY000 | regexp internal error |
| 3688 | ER_REGEXP_RULE_SYNTAX | DataError | 8.4: HY000, 9.7: HY000 | A regular-expression rule has invalid syntax |
| 3689 | ER_REGEXP_BAD_ESCAPE_SEQUENCE | DataError | 8.4: HY000, 9.7: HY000 | regexp bad escape sequence |
| 3690 | ER_REGEXP_UNIMPLEMENTED | NotSupportedError | 8.4: HY000, 9.7: HY000 | regexp unimplemented |
| 3691 | ER_REGEXP_MISMATCHED_PAREN | DataError | 8.4: HY000, 9.7: HY000 | A regular expression has unbalanced parentheses |
| 3692 | ER_REGEXP_BAD_INTERVAL | DataError | 8.4: HY000, 9.7: HY000 | A regular expression has an invalid repetition interval |
| 3693 | ER_REGEXP_MAX_LT_MIN | DataError | 8.4: HY000, 9.7: HY000 | A regular-expression repetition maximum is below its minimum |
| 3694 | ER_REGEXP_INVALID_BACK_REF | DataError | 8.4: HY000, 9.7: HY000 | regexp invalid back reference |
| 3695 | ER_REGEXP_LOOK_BEHIND_LIMIT | DataError | 8.4: HY000, 9.7: HY000 | A regular-expression lookbehind exceeds its supported length |
| 3696 | ER_REGEXP_MISSING_CLOSE_BRACKET | DataError | 8.4: HY000, 9.7: HY000 | A regular-expression character class lacks its closing bracket |
| 3697 | ER_REGEXP_INVALID_RANGE | DataError | 8.4: HY000, 9.7: HY000 | regexp invalid range |
| 3698 | ER_REGEXP_STACK_OVERFLOW | OperationalError | 8.4: HY000, 9.7: HY000 | regexp stack overflow |
| 3699 | ER_REGEXP_TIME_OUT | OperationalError | 8.4: HY000, 9.7: HY000 | Regular-expression evaluation exceeded its time limit |
| 3700 | ER_REGEXP_PATTERN_TOO_BIG | DataError | 8.4: HY000, 9.7: HY000 | A regular-expression pattern exceeds its supported size |
| 3701 | ER_CANT_SET_ERROR_LOG_SERVICE | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot set error log service |
| 3702 | ER_EMPTY_PIPELINE_FOR_ERROR_LOG_SERVICE | DatabaseError | 8.4: HY000, 9.7: HY000 | An empty error-log service pipeline disables error logging |
| 3703 | ER_COMPONENT_FILTER_DIAGNOSTICS | DatabaseError | 8.4: HY000, 9.7: HY000 | The error-log filter component reports the diagnostics in the server message |
| 3704 | ER_NOT_IMPLEMENTED_FOR_CARTESIAN_SRS | NotSupportedError | 8.4: 22S00, 9.7: 22S00 | not implemented for cartesian spatial reference system |
| 3705 | ER_NOT_IMPLEMENTED_FOR_PROJECTED_SRS | NotSupportedError | 8.4: 22S00, 9.7: 22S00 | not implemented for projected spatial reference system |
| 3706 | ER_NONPOSITIVE_RADIUS | DataError | 8.4: 22003, 9.7: 22003 | nonpositive radius |
| 3707 | ER_RESTART_SERVER_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | restart server failed |
| 3708 | ER_SRS_MISSING_MANDATORY_ATTRIBUTE | DataError | 8.4: SR006, 9.7: SR006 | spatial reference system missing mandatory attribute |
| 3709 | ER_SRS_MULTIPLE_ATTRIBUTE_DEFINITIONS | ProgrammingError | 8.4: SR006, 9.7: SR006 | spatial reference system multiple attribute definitions |
| 3710 | ER_SRS_NAME_CANT_BE_EMPTY_OR_WHITESPACE | ProgrammingError | 8.4: SR006, 9.7: SR006 | spatial reference system name cannot be empty or whitespace |
| 3711 | ER_SRS_ORGANIZATION_CANT_BE_EMPTY_OR_WHITESPACE | ProgrammingError | 8.4: SR006, 9.7: SR006 | spatial reference system organization cannot be empty or whitespace |
| 3712 | ER_SRS_ID_ALREADY_EXISTS | ProgrammingError | 8.4: SR004, 9.7: SR004 | spatial reference system id already exists |
| 3713 | ER_WARN_SRS_ID_ALREADY_EXISTS | ProgrammingError | 8.4: 01S00, 9.7: 01S00 | warn spatial reference system id already exists |
| 3714 | ER_CANT_MODIFY_SRID_0 | ProgrammingError | 8.4: SR000, 9.7: SR000 | Spatial reference identifier 0 cannot be modified |
| 3715 | ER_WARN_RESERVED_SRID_RANGE | DatabaseError | 8.4: 01S01, 9.7: 01S01 | The spatial reference identifier is in a reserved range |
| 3716 | ER_CANT_MODIFY_SRS_USED_BY_COLUMN | ProgrammingError | 8.4: SR005, 9.7: SR005 | cannot modify spatial reference system used by column |
| 3717 | ER_SRS_INVALID_CHARACTER_IN_ATTRIBUTE | DataError | 8.4: SR006, 9.7: SR006 | spatial reference system invalid character in attribute |
| 3718 | ER_SRS_ATTRIBUTE_STRING_TOO_LONG | DataError | 8.4: SR006, 9.7: SR006 | spatial reference system attribute string too long |
| 3719 | ER_DEPRECATED_UTF8_ALIAS | DatabaseError | 8.4: HY000, 9.7: HY000 | The utf8 alias for the three-byte character set is deprecated |
| 3720 | ER_DEPRECATED_NATIONAL | DatabaseError | 8.4: HY000, 9.7: HY000 | The national character-set syntax uses a deprecated character set |
| 3721 | ER_INVALID_DEFAULT_UTF8MB4_COLLATION | ProgrammingError | 8.4: HY000, 9.7: HY000 | invalid default utf8mb4 collation |
| 3722 | ER_UNABLE_TO_COLLECT_LOG_STATUS | OperationalError | 8.4: HY000, 9.7: HY000 | unable to collect log status |
| 3723 | ER_RESERVED_TABLESPACE_NAME | ProgrammingError | 8.4: HY000, 9.7: HY000 | reserved tablespace name |
| 3724 | ER_UNABLE_TO_SET_OPTION | ProgrammingError | 8.4: HY000, 9.7: HY000 | unable to set option |
| 3725 | ER_REPLICA_POSSIBLY_DIVERGED_AFTER_DDL | OperationalError | 8.4: HY000, 9.7: HY000 | Replication may have diverged after the schema-changing statement |
| 3726 | ER_SRS_NOT_GEOGRAPHIC | DataError | 8.4: 22S00, 9.7: 22S00 | spatial reference system not geographic |
| 3727 | ER_POLYGON_TOO_LARGE | DataError | 8.4: 22023, 9.7: 22023 | polygon too large |
| 3728 | ER_SPATIAL_UNIQUE_INDEX | ProgrammingError | 8.4: HY000, 9.7: HY000 | A spatial index cannot be declared UNIQUE |
| 3729 | ER_INDEX_TYPE_NOT_SUPPORTED_FOR_SPATIAL_INDEX | NotSupportedError | 8.4: HY000, 9.7: HY000 | index type not supported for spatial index |
| 3730 | ER_FK_CANNOT_DROP_PARENT | IntegrityError | 8.4: HY000, 9.7: HY000 | A table referenced by a foreign key cannot be dropped |
| 3731 | ER_GEOMETRY_PARAM_LONGITUDE_OUT_OF_RANGE | DataError | 8.4: 22S02, 9.7: 22S02 | A geometry argument has a longitude outside the spatial reference system's range |
| 3732 | ER_GEOMETRY_PARAM_LATITUDE_OUT_OF_RANGE | DataError | 8.4: 22S03, 9.7: 22S03 | A geometry argument has a latitude outside the spatial reference system's range |
| 3733 | ER_FK_CANNOT_USE_VIRTUAL_COLUMN | NotSupportedError | 8.4: HY000, 9.7: HY000 | foreign key cannot use virtual column |
| 3734 | ER_FK_NO_COLUMN_PARENT | ProgrammingError | 8.4: HY000, 9.7: HY000 | The referenced parent column does not exist |
| 3735 | ER_CANT_SET_ERROR_SUPPRESSION_LIST | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot set error suppression list |
| 3736 | ER_SRS_GEOGCS_INVALID_AXES | DataError | 8.4: SR002, 9.7: SR002 | The geographic spatial reference system has invalid axis definitions |
| 3737 | ER_SRS_INVALID_SEMI_MAJOR_AXIS | DataError | 8.4: SR002, 9.7: SR002 | spatial reference system invalid semi major axis |
| 3738 | ER_SRS_INVALID_INVERSE_FLATTENING | DataError | 8.4: SR002, 9.7: SR002 | spatial reference system invalid inverse flattening |
| 3739 | ER_SRS_INVALID_ANGULAR_UNIT | DataError | 8.4: SR002, 9.7: SR002 | spatial reference system invalid angular unit |
| 3740 | ER_SRS_INVALID_PRIME_MERIDIAN | DataError | 8.4: SR002, 9.7: SR002 | spatial reference system invalid prime meridian |
| 3741 | ER_TRANSFORM_SOURCE_SRS_NOT_SUPPORTED | NotSupportedError | 8.4: 22S00, 9.7: 22S00 | transform source spatial reference system not supported |
| 3742 | ER_TRANSFORM_TARGET_SRS_NOT_SUPPORTED | NotSupportedError | 8.4: 22S00, 9.7: 22S00 | transform target spatial reference system not supported |
| 3743 | ER_TRANSFORM_SOURCE_SRS_MISSING_TOWGS84 | DataError | 8.4: 22S00, 9.7: 22S00 | The source spatial reference system lacks the parameters required to transform to WGS 84 |
| 3744 | ER_TRANSFORM_TARGET_SRS_MISSING_TOWGS84 | DataError | 8.4: 22S00, 9.7: 22S00 | The target spatial reference system lacks the parameters required to transform from WGS 84 |
| 3745 | ER_TEMP_TABLE_PREVENTS_SWITCH_SESSION_BINLOG_FORMAT | OperationalError | 8.4: HY000, 9.7: HY000 | Open temporary tables prevent changing the session binary log format |
| 3746 | ER_TEMP_TABLE_PREVENTS_SWITCH_GLOBAL_BINLOG_FORMAT | OperationalError | 8.4: HY000, 9.7: HY000 | Open temporary tables prevent changing the global binary log format |
| 3747 | ER_RUNNING_APPLIER_PREVENTS_SWITCH_GLOBAL_BINLOG_FORMAT | OperationalError | 8.4: HY000, 9.7: HY000 | A running replication applier prevents changing the global binary log format |
| 3748 | ER_CLIENT_GTID_UNSAFE_CREATE_DROP_TEMP_TABLE_IN_TRX_IN_SBR | ProgrammingError | 8.4: HY000, 9.7: HY000 | Creating or dropping a temporary table in this transaction is unsafe for statement-based global-identifier replication |
| 3750 | ER_TABLE_WITHOUT_PK | ProgrammingError | 8.4: HY000, 9.7: HY000 | table without primary key |
| 3751 | ER_WARN_DATA_TRUNCATED_FUNCTIONAL_INDEX | DataError | 8.4: 01000, 9.7: 01000 | A functional-index value was truncated |
| 3752 | ER_WARN_DATA_OUT_OF_RANGE_FUNCTIONAL_INDEX | DataError | 8.4: 22003, 9.7: 22003 | A functional-index value is outside its supported range |
| 3753 | ER_FUNCTIONAL_INDEX_ON_JSON_OR_GEOMETRY_FUNCTION | ProgrammingError | 8.4: 42000, 9.7: 42000 | A functional index cannot directly index a JSON or geometry result |
| 3754 | ER_FUNCTIONAL_INDEX_REF_AUTO_INCREMENT | ProgrammingError | 8.4: HY000, 9.7: HY000 | A functional index cannot reference an AUTO_INCREMENT column |
| 3755 | ER_CANNOT_DROP_COLUMN_FUNCTIONAL_INDEX | ProgrammingError | 8.4: HY000, 9.7: HY000 | A column used by a functional index cannot be dropped |
| 3756 | ER_FUNCTIONAL_INDEX_PRIMARY_KEY | ProgrammingError | 8.4: HY000, 9.7: HY000 | A primary key cannot be a functional index |
| 3757 | ER_FUNCTIONAL_INDEX_ON_LOB | ProgrammingError | 8.4: HY000, 9.7: HY000 | A functional index cannot return a large-object type |
| 3758 | ER_FUNCTIONAL_INDEX_FUNCTION_IS_NOT_ALLOWED | ProgrammingError | 8.4: HY000, 9.7: HY000 | functional index function is not allowed |
| 3759 | ER_FULLTEXT_FUNCTIONAL_INDEX | NotSupportedError | 8.4: HY000, 9.7: HY000 | A FULLTEXT index cannot be functional |
| 3760 | ER_SPATIAL_FUNCTIONAL_INDEX | NotSupportedError | 8.4: HY000, 9.7: HY000 | A spatial index cannot be functional |
| 3761 | ER_WRONG_KEY_COLUMN_FUNCTIONAL_INDEX | NotSupportedError | 8.4: HY000, 9.7: HY000 | The functional index expression has an invalid result column type |
| 3762 | ER_FUNCTIONAL_INDEX_ON_FIELD | ProgrammingError | 8.4: HY000, 9.7: HY000 | A functional index expression cannot consist of a bare column reference |
| 3763 | ER_GENERATED_COLUMN_NAMED_FUNCTION_IS_NOT_ALLOWED | ProgrammingError | 8.4: HY000, 9.7: HY000 | A generated column cannot use the specified named function |
| 3764 | ER_GENERATED_COLUMN_ROW_VALUE | ProgrammingError | 8.4: HY000, 9.7: HY000 | A generated column expression cannot produce a row value |
| 3765 | ER_GENERATED_COLUMN_VARIABLES | ProgrammingError | 8.4: HY000, 9.7: HY000 | A generated column expression cannot reference variables |
| 3766 | ER_DEPENDENT_BY_DEFAULT_GENERATED_VALUE | ProgrammingError | 8.4: HY000, 9.7: HY000 | Another column's default expression depends on this column |
| 3767 | ER_DEFAULT_VAL_GENERATED_NON_PRIOR | ProgrammingError | 8.4: HY000, 9.7: HY000 | A default expression cannot refer to a later column with a generated default |
| 3768 | ER_DEFAULT_VAL_GENERATED_REF_AUTO_INC | ProgrammingError | 8.4: HY000, 9.7: HY000 | A default expression cannot reference an AUTO_INCREMENT column |
| 3769 | ER_DEFAULT_VAL_GENERATED_FUNCTION_IS_NOT_ALLOWED | ProgrammingError | 8.4: HY000, 9.7: HY000 | The function is not allowed in a default expression |
| 3770 | ER_DEFAULT_VAL_GENERATED_NAMED_FUNCTION_IS_NOT_ALLOWED | ProgrammingError | 8.4: HY000, 9.7: HY000 | A named function is not allowed in a default expression |
| 3771 | ER_DEFAULT_VAL_GENERATED_ROW_VALUE | ProgrammingError | 8.4: HY000, 9.7: HY000 | A default expression cannot produce a row value |
| 3772 | ER_DEFAULT_VAL_GENERATED_VARIABLES | ProgrammingError | 8.4: HY000, 9.7: HY000 | A default expression cannot reference variables |
| 3773 | ER_DEFAULT_AS_VAL_GENERATED | ProgrammingError | 8.4: HY000, 9.7: HY000 | DEFAULT cannot be used as this generated expression's value |
| 3774 | ER_UNSUPPORTED_ACTION_ON_DEFAULT_VAL_GENERATED | NotSupportedError | 8.4: HY000, 9.7: HY000 | The requested operation is unsupported for this generated default expression |
| 3775 | ER_GTID_UNSAFE_ALTER_ADD_COL_WITH_DEFAULT_EXPRESSION | ProgrammingError | 8.4: HY000, 9.7: HY000 | Adding a column with this default expression would violate global transaction identifier consistency |
| 3776 | ER_FK_CANNOT_CHANGE_ENGINE | IntegrityError | 8.4: HY000, 9.7: HY000 | The storage engine cannot be changed while the table has foreign keys |
| 3777 | ER_WARN_DEPRECATED_USER_SET_EXPR | DatabaseError | 8.4: HY000, 9.7: HY000 | Assigning a user variable inside an expression is deprecated |
| 3778 | ER_WARN_DEPRECATED_UTF8MB3_COLLATION | DatabaseError | 8.4: HY000, 9.7: HY000 | The three-byte UTF-8 collation is deprecated |
| 3779 | ER_WARN_DEPRECATED_NESTED_COMMENT_SYNTAX | DatabaseError | 8.4: HY000, 9.7: HY000 | Nested comment syntax is deprecated |
| 3780 | ER_FK_INCOMPATIBLE_COLUMNS | ProgrammingError | 8.4: HY000, 9.7: HY000 | The referencing and referenced foreign key columns have incompatible definitions |
| 3781 | ER_GR_HOLD_WAIT_TIMEOUT | OperationalError | 8.4: HY000, 9.7: HY000 | Waiting for group replication to become ready timed out |
| 3782 | ER_GR_HOLD_KILLED | OperationalError | 8.4: HY000, 9.7: HY000 | Waiting for group replication was interrupted |
| 3783 | ER_GR_HOLD_MEMBER_STATUS_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | The member's group replication state does not permit this operation |
| 3784 | ER_RPL_ENCRYPTION_FAILED_TO_FETCH_KEY | OperationalError | 8.4: HY000, 9.7: HY000 | The replication encryption key could not be retrieved |
| 3785 | ER_RPL_ENCRYPTION_KEY_NOT_FOUND | OperationalError | 8.4: HY000, 9.7: HY000 | The replication encryption key does not exist |
| 3786 | ER_RPL_ENCRYPTION_KEYRING_INVALID_KEY | OperationalError | 8.4: HY000, 9.7: HY000 | The keyring returned an invalid replication encryption key |
| 3787 | ER_RPL_ENCRYPTION_HEADER_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | The encrypted replication log header is invalid |
| 3788 | ER_RPL_ENCRYPTION_FAILED_TO_ROTATE_LOGS | OperationalError | 8.4: HY000, 9.7: HY000 | The encrypted replication logs could not be rotated |
| 3789 | ER_RPL_ENCRYPTION_KEY_EXISTS_UNEXPECTED | InternalError | 8.4: HY000, 9.7: HY000 | A replication encryption key unexpectedly already exists |
| 3790 | ER_RPL_ENCRYPTION_FAILED_TO_GENERATE_KEY | OperationalError | 8.4: HY000, 9.7: HY000 | A replication encryption key could not be generated |
| 3791 | ER_RPL_ENCRYPTION_FAILED_TO_STORE_KEY | OperationalError | 8.4: HY000, 9.7: HY000 | A replication encryption key could not be stored |
| 3792 | ER_RPL_ENCRYPTION_FAILED_TO_REMOVE_KEY | OperationalError | 8.4: HY000, 9.7: HY000 | A replication encryption key could not be removed |
| 3793 | ER_RPL_ENCRYPTION_UNABLE_TO_CHANGE_OPTION | OperationalError | 8.4: HY000, 9.7: HY000 | The replication encryption option could not be changed |
| 3794 | ER_RPL_ENCRYPTION_MASTER_KEY_RECOVERY_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | The replication encryption master key could not be recovered |
| 3795 | ER_SLOW_LOG_MODE_IGNORED_WHEN_NOT_LOGGING_TO_FILE | DatabaseError | 8.4: HY000, 9.7: HY000 | slow log mode ignored when not logging to file |
| 3796 | ER_GRP_TRX_CONSISTENCY_NOT_ALLOWED | OperationalError | 8.4: HY000, 9.7: HY000 | The requested group replication transaction consistency mode is not allowed |
| 3797 | ER_GRP_TRX_CONSISTENCY_BEFORE | OperationalError | 8.4: HY000, 9.7: HY000 | The group replication consistency wait before the transaction failed |
| 3798 | ER_GRP_TRX_CONSISTENCY_AFTER_ON_TRX_BEGIN | OperationalError | 8.4: HY000, 9.7: HY000 | The group replication consistency wait when beginning a transaction failed |
| 3799 | ER_GRP_TRX_CONSISTENCY_BEGIN_NOT_ALLOWED | OperationalError | 8.4: HY000, 9.7: HY000 | The current state does not allow beginning a transaction with this group consistency mode |
| 3800 | ER_FUNCTIONAL_INDEX_ROW_VALUE_IS_NOT_ALLOWED | ProgrammingError | 8.4: HY000, 9.7: HY000 | A functional index expression cannot produce a row value |
| 3801 | ER_RPL_ENCRYPTION_FAILED_TO_ENCRYPT | OperationalError | 8.4: HY000, 9.7: HY000 | The replication log could not be encrypted |
| 3802 | ER_PAGE_TRACKING_NOT_STARTED | OperationalError | 8.4: HY000, 9.7: HY000 | page tracking not started |
| 3803 | ER_PAGE_TRACKING_RANGE_NOT_TRACKED | OperationalError | 8.4: HY000, 9.7: HY000 | page tracking range not tracked |
| 3804 | ER_PAGE_TRACKING_CANNOT_PURGE | OperationalError | 8.4: HY000, 9.7: HY000 | page tracking cannot purge |
| 3805 | ER_RPL_ENCRYPTION_CANNOT_ROTATE_BINLOG_MASTER_KEY | OperationalError | 8.4: HY000, 9.7: HY000 | The binary log encryption master key could not be rotated |
| 3806 | ER_BINLOG_MASTER_KEY_RECOVERY_OUT_OF_COMBINATION | InternalError | 8.4: HY000, 9.7: HY000 | The binary log encryption master-key recovery state is inconsistent |
| 3807 | ER_BINLOG_MASTER_KEY_ROTATION_FAIL_TO_OPERATE_KEY | OperationalError | 8.4: HY000, 9.7: HY000 | The keyring operation required for binary log master-key rotation failed |
| 3808 | ER_BINLOG_MASTER_KEY_ROTATION_FAIL_TO_ROTATE_LOGS | OperationalError | 8.4: HY000, 9.7: HY000 | Rotating the logs during binary log master-key rotation failed |
| 3809 | ER_BINLOG_MASTER_KEY_ROTATION_FAIL_TO_REENCRYPT_LOG | OperationalError | 8.4: HY000, 9.7: HY000 | Reencrypting a log during binary log master-key rotation failed |
| 3810 | ER_BINLOG_MASTER_KEY_ROTATION_FAIL_TO_CLEANUP_UNUSED_KEYS | OperationalError | 8.4: HY000, 9.7: HY000 | Unused keys could not be removed after binary log master-key rotation |
| 3811 | ER_BINLOG_MASTER_KEY_ROTATION_FAIL_TO_CLEANUP_AUX_KEY | OperationalError | 8.4: HY000, 9.7: HY000 | The auxiliary key could not be removed after binary log master-key rotation |
| 3812 | ER_NON_BOOLEAN_EXPR_FOR_CHECK_CONSTRAINT | ProgrammingError | 8.4: HY000, 9.7: HY000 | A CHECK constraint expression must be boolean |
| 3813 | ER_COLUMN_CHECK_CONSTRAINT_REFERENCES_OTHER_COLUMN | ProgrammingError | 8.4: HY000, 9.7: HY000 | A column-level CHECK constraint cannot reference another column |
| 3814 | ER_CHECK_CONSTRAINT_NAMED_FUNCTION_IS_NOT_ALLOWED | ProgrammingError | 8.4: HY000, 9.7: HY000 | A CHECK constraint cannot use the specified named function |
| 3815 | ER_CHECK_CONSTRAINT_FUNCTION_IS_NOT_ALLOWED | ProgrammingError | 8.4: HY000, 9.7: HY000 | A CHECK constraint cannot use the specified function |
| 3816 | ER_CHECK_CONSTRAINT_VARIABLES | ProgrammingError | 8.4: HY000, 9.7: HY000 | A CHECK constraint cannot reference variables |
| 3817 | ER_CHECK_CONSTRAINT_ROW_VALUE | ProgrammingError | 8.4: HY000, 9.7: HY000 | A CHECK constraint cannot produce a row value |
| 3818 | ER_CHECK_CONSTRAINT_REFERS_AUTO_INCREMENT_COLUMN | ProgrammingError | 8.4: HY000, 9.7: HY000 | A CHECK constraint cannot reference an AUTO_INCREMENT column |
| 3819 | ER_CHECK_CONSTRAINT_VIOLATED | IntegrityError | 8.4: HY000, 9.7: HY000 | check constraint violated |
| 3820 | ER_CHECK_CONSTRAINT_REFERS_UNKNOWN_COLUMN | ProgrammingError | 8.4: HY000, 9.7: HY000 | A CHECK constraint references an unknown column |
| 3821 | ER_CHECK_CONSTRAINT_NOT_FOUND | ProgrammingError | 8.4: HY000, 9.7: HY000 | The named CHECK constraint does not exist |
| 3822 | ER_CHECK_CONSTRAINT_DUP_NAME | ProgrammingError | 8.4: HY000, 9.7: HY000 | A CHECK constraint name is already in use |
| 3823 | ER_CHECK_CONSTRAINT_CLAUSE_USING_FK_REFER_ACTION_COLUMN | ProgrammingError | 8.4: HY000, 9.7: HY000 | A CHECK constraint uses a column affected by an incompatible foreign key action |
| 3824 | WARN_UNENCRYPTED_TABLE_IN_ENCRYPTED_DB | DatabaseError | 8.4: HY000, 9.7: HY000 | unencrypted table in encrypted database |
| 3825 | ER_INVALID_ENCRYPTION_REQUEST | ProgrammingError | 8.4: HY000, 9.7: HY000 | invalid encryption request |
| 3826 | ER_CANNOT_SET_TABLE_ENCRYPTION | OperationalError | 8.4: HY000, 9.7: HY000 | cannot set table encryption |
| 3827 | ER_CANNOT_SET_DATABASE_ENCRYPTION | OperationalError | 8.4: HY000, 9.7: HY000 | cannot set database encryption |
| 3828 | ER_CANNOT_SET_TABLESPACE_ENCRYPTION | OperationalError | 8.4: HY000, 9.7: HY000 | cannot set tablespace encryption |
| 3829 | ER_TABLESPACE_CANNOT_BE_ENCRYPTED | OperationalError | 8.4: HY000, 9.7: HY000 | tablespace cannot be encrypted |
| 3830 | ER_TABLESPACE_CANNOT_BE_DECRYPTED | OperationalError | 8.4: HY000, 9.7: HY000 | tablespace cannot be decrypted |
| 3831 | ER_TABLESPACE_TYPE_UNKNOWN | DatabaseError | 8.4: HY000, 9.7: HY000 | tablespace type unknown |
| 3832 | ER_TARGET_TABLESPACE_UNENCRYPTED | ProgrammingError | 8.4: HY000, 9.7: HY000 | target tablespace unencrypted |
| 3833 | ER_CANNOT_USE_ENCRYPTION_CLAUSE | ProgrammingError | 8.4: HY000, 9.7: HY000 | cannot use encryption clause |
| 3834 | ER_INVALID_MULTIPLE_CLAUSES | ProgrammingError | 8.4: HY000, 9.7: HY000 | The statement contains incompatible or duplicate clauses |
| 3835 | ER_UNSUPPORTED_USE_OF_GRANT_AS | NotSupportedError | 8.4: HY000, 9.7: HY000 | This use of GRANT ... AS is not supported |
| 3836 | ER_UKNOWN_AUTH_ID_OR_ACCESS_DENIED_FOR_GRANT_AS | DatabaseError | 8.4: HY000, 9.7: HY000 | GRANT ... AS names an unknown account or an account that cannot be used for authorization |
| 3837 | ER_DEPENDENT_BY_FUNCTIONAL_INDEX | ProgrammingError | 8.4: HY000, 9.7: HY000 | A functional index depends on the object being changed |
| 3838 | ER_PLUGIN_NOT_EARLY | ProgrammingError | 8.4: HY000, 9.7: HY000 | The plugin must not be loaded through the early-plugin interface |
| 3839 | ER_INNODB_REDO_LOG_ARCHIVE_START_SUBDIR_PATH | ProgrammingError | 8.4: HY000, 9.7: HY000 | The InnoDB redo-log archive subdirectory is invalid |
| 3840 | ER_INNODB_REDO_LOG_ARCHIVE_START_TIMEOUT | OperationalError | 8.4: HY000, 9.7: HY000 | Starting the InnoDB redo-log archive timed out |
| 3841 | ER_INNODB_REDO_LOG_ARCHIVE_DIRS_INVALID | OperationalError | 8.4: HY000, 9.7: HY000 | The configured InnoDB redo-log archive directories are invalid |
| 3842 | ER_INNODB_REDO_LOG_ARCHIVE_LABEL_NOT_FOUND | ProgrammingError | 8.4: HY000, 9.7: HY000 | The requested redo-log archive directory label is unknown |
| 3843 | ER_INNODB_REDO_LOG_ARCHIVE_DIR_EMPTY | OperationalError | 8.4: HY000, 9.7: HY000 | The redo-log archive directory specification is empty |
| 3844 | ER_INNODB_REDO_LOG_ARCHIVE_NO_SUCH_DIR | OperationalError | 8.4: HY000, 9.7: HY000 | The redo-log archive directory does not exist |
| 3845 | ER_INNODB_REDO_LOG_ARCHIVE_DIR_CLASH | OperationalError | 8.4: HY000, 9.7: HY000 | Redo-log archive directory definitions conflict |
| 3846 | ER_INNODB_REDO_LOG_ARCHIVE_DIR_PERMISSIONS | OperationalError | 8.4: HY000, 9.7: HY000 | The redo-log archive directory has unsafe or insufficient permissions |
| 3847 | ER_INNODB_REDO_LOG_ARCHIVE_FILE_CREATE | OperationalError | 8.4: HY000, 9.7: HY000 | A redo-log archive file could not be created |
| 3848 | ER_INNODB_REDO_LOG_ARCHIVE_ACTIVE | OperationalError | 8.4: HY000, 9.7: HY000 | Redo-log archiving is already active |
| 3849 | ER_INNODB_REDO_LOG_ARCHIVE_INACTIVE | OperationalError | 8.4: HY000, 9.7: HY000 | Redo-log archiving is not active |
| 3850 | ER_INNODB_REDO_LOG_ARCHIVE_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | Redo-log archiving failed |
| 3851 | ER_INNODB_REDO_LOG_ARCHIVE_SESSION | ProgrammingError | 8.4: HY000, 9.7: HY000 | Redo-log archiving cannot be controlled by this session |
| 3852 | ER_STD_REGEX_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | The native standard-library regular-expression engine reported an error |
| 3853 | ER_INVALID_JSON_TYPE | DataError | 8.4: 22032, 9.7: 22032 | invalid JSON type |
| 3854 | ER_CANNOT_CONVERT_STRING | DataError | 8.4: HY000, 9.7: HY000 | cannot convert string |
| 3855 | ER_DEPENDENT_BY_PARTITION_FUNC | ProgrammingError | 8.4: HY000, 9.7: HY000 | A partitioning function depends on the column being changed |
| 3856 | ER_WARN_DEPRECATED_FLOAT_AUTO_INCREMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | AUTO_INCREMENT on a floating-point column is deprecated |
| 3857 | ER_RPL_CANT_STOP_REPLICA_WHILE_LOCKED_BACKUP | OperationalError | 8.4: HY000, 9.7: HY000 | The replica cannot be stopped while a backup lock is held |
| 3858 | ER_WARN_DEPRECATED_FLOAT_DIGITS | DatabaseError | 8.4: HY000, 9.7: HY000 | The floating-point precision and scale syntax is deprecated |
| 3859 | ER_WARN_DEPRECATED_FLOAT_UNSIGNED | DatabaseError | 8.4: HY000, 9.7: HY000 | UNSIGNED on a floating-point column is deprecated |
| 3860 | ER_WARN_DEPRECATED_INTEGER_DISPLAY_WIDTH | DatabaseError | 8.4: HY000, 9.7: HY000 | Integer display widths are deprecated |
| 3861 | ER_WARN_DEPRECATED_ZEROFILL | DatabaseError | 8.4: HY000, 9.7: HY000 | ZEROFILL is deprecated |
| 3862 | ER_CLONE_DONOR | OperationalError | 8.4: HY000, 9.7: HY000 | The clone donor reported an error |
| 3863 | ER_CLONE_PROTOCOL | OperationalError | 8.4: HY000, 9.7: HY000 | The clone protocol exchange is invalid or incompatible |
| 3864 | ER_CLONE_DONOR_VERSION | NotSupportedError | 8.4: HY000, 9.7: HY000 | The clone donor uses an incompatible server version |
| 3865 | ER_CLONE_OS | NotSupportedError | 8.4: HY000, 9.7: HY000 | The clone donor and recipient use incompatible operating systems |
| 3866 | ER_CLONE_PLATFORM | NotSupportedError | 8.4: HY000, 9.7: HY000 | The clone donor and recipient use incompatible hardware platforms |
| 3867 | ER_CLONE_CHARSET | NotSupportedError | 8.4: HY000, 9.7: HY000 | The clone donor and recipient use incompatible character-set settings |
| 3868 | ER_CLONE_CONFIG | OperationalError | 8.4: HY000, 9.7: HY000 | The clone donor and recipient have incompatible server configuration |
| 3869 | ER_CLONE_SYS_CONFIG | OperationalError | 8.4: HY000, 9.7: HY000 | The clone donor and recipient have incompatible system configuration |
| 3870 | ER_CLONE_PLUGIN_MATCH | OperationalError | 8.4: HY000, 9.7: HY000 | The clone donor and recipient have incompatible plugin sets |
| 3871 | ER_CLONE_LOOPBACK | ProgrammingError | 8.4: HY000, 9.7: HY000 | The clone donor cannot be the recipient itself |
| 3872 | ER_CLONE_ENCRYPTION | OperationalError | 8.4: HY000, 9.7: HY000 | The clone encryption configuration is incompatible |
| 3873 | ER_CLONE_DISK_SPACE | OperationalError | 8.4: HY000, 9.7: HY000 | The clone recipient has insufficient disk space |
| 3874 | ER_CLONE_IN_PROGRESS | OperationalError | 8.4: HY000, 9.7: HY000 | A clone operation is already in progress |
| 3875 | ER_CLONE_DISALLOWED | OperationalError | 8.4: HY000, 9.7: HY000 | Cloning is not permitted in the current state |
| 3876 | ER_CANNOT_GRANT_ROLES_TO_ANONYMOUS_USER | ProgrammingError | 8.4: HY000, 9.7: HY000 | cannot grant roles to anonymous user |
| 3877 | ER_SECONDARY_ENGINE_PLUGIN | DatabaseError | 8.4: HY000, 9.7: HY000 | The secondary storage engine plugin reports the condition in the server message |
| 3878 | ER_SECOND_PASSWORD_CANNOT_BE_EMPTY | ProgrammingError | 8.4: HY000, 9.7: HY000 | second password cannot be empty |
| 3879 | ER_DB_ACCESS_DENIED | OperationalError | 8.4: HY000, 9.7: HY000 | database access denied |
| 3880 | ER_DA_AUTH_ID_WITH_SYSTEM_USER_PRIV_IN_MANDATORY_ROLES | ProgrammingError | 8.4: HY000, 9.7: HY000 | A mandatory role cannot have an account with SYSTEM_USER privileges |
| 3881 | ER_DA_RPL_GTID_TABLE_CANNOT_OPEN | OperationalError | 8.4: HY000, 9.7: HY000 | The replication global transaction identifier table could not be opened |
| 3882 | ER_GEOMETRY_IN_UNKNOWN_LENGTH_UNIT | DataError | 8.4: SU001, 9.7: SU001 | The geometry uses an unknown length unit |
| 3883 | ER_DA_PLUGIN_INSTALL_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | Installing the plugin failed |
| 3884 | ER_NO_SESSION_TEMP | OperationalError | 8.4: HY000, 9.7: HY000 | The session has no temporary tablespace |
| 3885 | ER_DA_UNKNOWN_ERROR_NUMBER | DatabaseError | 8.4: HY000, 9.7: HY000 | The diagnostics area refers to an unknown error number |
| 3886 | ER_COLUMN_CHANGE_SIZE | ProgrammingError | 8.4: HY000, 9.7: HY000 | The requested column size change is invalid |
| 3887 | ER_REGEXP_INVALID_CAPTURE_GROUP_NAME | DataError | 8.4: HY000, 9.7: HY000 | regexp invalid capture group name |
| 3888 | ER_DA_SSL_LIBRARY_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | The TLS library reported the error described by the server message |
| 3889 | ER_SECONDARY_ENGINE | DatabaseError | 8.4: HY000, 9.7: HY000 | The secondary storage engine rejected or failed the operation |
| 3890 | ER_SECONDARY_ENGINE_DDL | NotSupportedError | 8.4: HY000, 9.7: HY000 | The secondary storage engine does not support this schema-changing operation |
| 3891 | ER_INCORRECT_CURRENT_PASSWORD | OperationalError | 8.4: HY000, 9.7: HY000 | incorrect current password |
| 3892 | ER_MISSING_CURRENT_PASSWORD | ProgrammingError | 8.4: HY000, 9.7: HY000 | missing current password |
| 3893 | ER_CURRENT_PASSWORD_NOT_REQUIRED | ProgrammingError | 8.4: HY000, 9.7: HY000 | current password not required |
| 3894 | ER_PASSWORD_CANNOT_BE_RETAINED_ON_PLUGIN_CHANGE | ProgrammingError | 8.4: HY000, 9.7: HY000 | password cannot be retained on plugin change |
| 3895 | ER_CURRENT_PASSWORD_CANNOT_BE_RETAINED | ProgrammingError | 8.4: HY000, 9.7: HY000 | current password cannot be retained |
| 3896 | ER_PARTIAL_REVOKES_EXIST | OperationalError | 8.4: HY000, 9.7: HY000 | partial revokes exist |
| 3897 | ER_CANNOT_GRANT_SYSTEM_PRIV_TO_MANDATORY_ROLE | ProgrammingError | 8.4: HY000, 9.7: HY000 | SYSTEM_USER privilege cannot be granted to a mandatory role |
| 3898 | ER_XA_REPLICATION_FILTERS | NotSupportedError | 8.4: HY000, 9.7: HY000 | Distributed-transaction replication cannot use the configured replication filters |
| 3899 | ER_UNSUPPORTED_SQL_MODE | NotSupportedError | 8.4: HY000, 9.7: HY000 | unsupported SQL mode |
| 3900 | ER_REGEXP_INVALID_FLAG | DataError | 8.4: HY000, 9.7: HY000 | regexp invalid flag |
| 3901 | ER_PARTIAL_REVOKE_AND_DB_GRANT_BOTH_EXISTS | InternalError | 8.4: HY000, 9.7: HY000 | Partial database revokes conflict with the requested database grant |
| 3902 | ER_UNIT_NOT_FOUND | ProgrammingError | 8.4: SU001, 9.7: SU001 | unit not found |
| 3903 | ER_INVALID_JSON_VALUE_FOR_FUNC_INDEX | DataError | 8.4: 22018, 9.7: 22018 | A JSON value cannot be stored in the functional index |
| 3904 | ER_JSON_VALUE_OUT_OF_RANGE_FOR_FUNC_INDEX | DataError | 8.4: 22003, 9.7: 22003 | A JSON value exceeds the functional index's supported range |
| 3905 | ER_EXCEEDED_MV_KEYS_NUM | DataError | 8.4: HY000, 9.7: HY000 | A multi-valued index has too many keys for one record |
| 3906 | ER_EXCEEDED_MV_KEYS_SPACE | DataError | 8.4: HY000, 9.7: HY000 | A multi-valued index's keys exceed the per-record space limit |
| 3907 | ER_FUNCTIONAL_INDEX_DATA_IS_TOO_LONG | DataError | 8.4: 22001, 9.7: 22001 | functional index data is too long |
| 3908 | ER_WRONG_MVI_VALUE | DataError | 8.4: HY000, 9.7: HY000 | A value is invalid for a multi-valued index |
| 3909 | ER_WARN_FUNC_INDEX_NOT_APPLICABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | The functional index cannot be used for this comparison |
| 3910 | ER_GRP_RPL_UDF_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | A group replication management function failed |
| 3911 | ER_UPDATE_GTID_PURGED_WITH_GR | OperationalError | 8.4: HY000, 9.7: HY000 | Purged global transaction identifiers cannot be changed while group replication is active |
| 3912 | ER_GROUPING_ON_TIMESTAMP_IN_DST | DatabaseError | 8.4: HY000, 9.7: HY000 | Grouping TIMESTAMP values is ambiguous across a daylight-saving transition |
| 3913 | ER_TABLE_NAME_CAUSES_TOO_LONG_PATH | ProgrammingError | 8.4: HY000, 9.7: HY000 | The table name would produce an excessively long filesystem path |
| 3914 | ER_AUDIT_LOG_INSUFFICIENT_PRIVILEGE | OperationalError | 8.4: HY000, 9.7: HY000 | audit log insufficient privilege |
| 3916 | ER_DA_GRP_RPL_STARTED_AUTO_REJOIN | DatabaseError | 8.4: HY000, 9.7: HY000 | Group replication started an automatic attempt to rejoin the group |
| 3917 | ER_SYSVAR_CHANGE_DURING_QUERY | OperationalError | 8.4: HY000, 9.7: HY000 | A system variable changed while the query was reading it |
| 3918 | ER_GLOBSTAT_CHANGE_DURING_QUERY | OperationalError | 8.4: HY000, 9.7: HY000 | Global statistics changed while the query was reading them |
| 3919 | ER_GRP_RPL_MESSAGE_SERVICE_INIT_FAILURE | OperationalError | 8.4: HY000, 9.7: HY000 | The group replication messaging service could not be initialized |
| 3920 | ER_CHANGE_SOURCE_WRONG_COMPRESSION_ALGORITHM_CLIENT | DataError | 8.4: HY000, 9.7: HY000 | The replication source connection specifies an invalid compression algorithm |
| 3921 | ER_CHANGE_SOURCE_WRONG_COMPRESSION_LEVEL_CLIENT | DataError | 8.4: HY000, 9.7: HY000 | The replication source connection specifies an invalid compression level |
| 3922 | ER_WRONG_COMPRESSION_ALGORITHM_CLIENT | DataError | 8.4: HY000, 9.7: HY000 | invalid compression algorithm client |
| 3923 | ER_WRONG_COMPRESSION_LEVEL_CLIENT | DataError | 8.4: HY000, 9.7: HY000 | invalid compression level client |
| 3924 | ER_CHANGE_SOURCE_WRONG_COMPRESSION_ALGORITHM_LIST_CLIENT | ProgrammingError | 8.4: HY000, 9.7: HY000 | The replication source connection specifies an invalid compression algorithm list |
| 3925 | ER_CLIENT_PRIVILEGE_CHECKS_USER_CANNOT_BE_ANONYMOUS | ProgrammingError | 8.4: HY000, 9.7: HY000 | The replication privilege-checking account cannot be anonymous |
| 3926 | ER_CLIENT_PRIVILEGE_CHECKS_USER_DOES_NOT_EXIST | ProgrammingError | 8.4: HY000, 9.7: HY000 | The replication privilege-checking account does not exist |
| 3927 | ER_CLIENT_PRIVILEGE_CHECKS_USER_CORRUPT | OperationalError | 8.4: HY000, 9.7: HY000 | The replication privilege-checking account has invalid metadata |
| 3928 | ER_CLIENT_PRIVILEGE_CHECKS_USER_NEEDS_RPL_APPLIER_PRIV | OperationalError | 8.4: HY000, 9.7: HY000 | The privilege-checking replication account lacks REPLICATION_APPLIER |
| 3929 | ER_WARN_DA_PRIVILEGE_NOT_REGISTERED | ProgrammingError | 8.4: HY000, 9.7: HY000 | The requested dynamic privilege is not registered |
| 3930 | ER_CLIENT_KEYRING_UDF_KEY_INVALID | DataError | 8.4: HY000, 9.7: HY000 | A keyring function received an invalid key identifier or value |
| 3931 | ER_CLIENT_KEYRING_UDF_KEY_TYPE_INVALID | DataError | 8.4: HY000, 9.7: HY000 | A keyring function received an invalid key type |
| 3932 | ER_CLIENT_KEYRING_UDF_KEY_TOO_LONG | DataError | 8.4: HY000, 9.7: HY000 | A keyring function's key value exceeds its supported length |
| 3933 | ER_CLIENT_KEYRING_UDF_KEY_TYPE_TOO_LONG | DataError | 8.4: HY000, 9.7: HY000 | A keyring function's key type exceeds its supported length |
| 3934 | ER_JSON_SCHEMA_VALIDATION_ERROR_WITH_DETAILED_REPORT | DataError | 8.4: HY000, 9.7: HY000 | JSON schema validation error with detailed report |
| 3935 | ER_DA_UDF_INVALID_CHARSET_SPECIFIED | ProgrammingError | 8.4: HY000, 9.7: HY000 | A loadable function specifies an invalid character-set name |
| 3936 | ER_DA_UDF_INVALID_CHARSET | ProgrammingError | 8.4: HY000, 9.7: HY000 | A loadable function uses an invalid character set |
| 3937 | ER_DA_UDF_INVALID_COLLATION | ProgrammingError | 8.4: HY000, 9.7: HY000 | A loadable function uses an invalid collation |
| 3938 | ER_DA_UDF_INVALID_EXTENSION_ARGUMENT_TYPE | ProgrammingError | 8.4: HY000, 9.7: HY000 | A loadable function received an invalid extension argument type |
| 3939 | ER_MULTIPLE_CONSTRAINTS_WITH_SAME_NAME | ProgrammingError | 8.4: HY000, 9.7: HY000 | multiple constraints with same name |
| 3940 | ER_CONSTRAINT_NOT_FOUND | ProgrammingError | 8.4: HY000, 9.7: HY000 | constraint not found |
| 3941 | ER_ALTER_CONSTRAINT_ENFORCEMENT_NOT_SUPPORTED | NotSupportedError | 8.4: HY000, 9.7: HY000 | alter constraint enforcement not supported |
| 3942 | ER_TABLE_VALUE_CONSTRUCTOR_MUST_HAVE_COLUMNS | ProgrammingError | 8.4: HY000, 9.7: HY000 | table value constructor must have columns |
| 3943 | ER_TABLE_VALUE_CONSTRUCTOR_CANNOT_HAVE_DEFAULT | ProgrammingError | 8.4: HY000, 9.7: HY000 | table value constructor cannot have default |
| 3944 | ER_CLIENT_QUERY_FAILURE_INVALID_NON_ROW_FORMAT | ProgrammingError | 8.4: HY000, 9.7: HY000 | The replication statement violates REQUIRE_ROW_FORMAT |
| 3945 | ER_REQUIRE_ROW_FORMAT_INVALID_VALUE | DataError | 8.4: HY000, 9.7: HY000 | require row format invalid value |
| 3946 | ER_FAILED_TO_DETERMINE_IF_ROLE_IS_MANDATORY | OperationalError | 8.4: HY000, 9.7: HY000 | failed to determine if role is mandatory |
| 3947 | ER_FAILED_TO_FETCH_MANDATORY_ROLE_LIST | OperationalError | 8.4: HY000, 9.7: HY000 | failed to fetch mandatory role list |
| 3948 | ER_CLIENT_LOCAL_FILES_DISABLED | NotSupportedError | 8.4: 42000, 9.7: 42000 | Loading a client-local file requires the capability on both client and server |
| 3949 | ER_IMP_INCOMPATIBLE_CFG_VERSION | NotSupportedError | 8.4: HY000, 9.7: HY000 | The import configuration file has an incompatible version |
| 3950 | ER_DA_OOM | OperationalError | 8.4: HY000, 9.7: HY000 | The server ran out of memory while producing diagnostics |
| 3951 | ER_DA_UDF_INVALID_ARGUMENT_TO_SET_CHARSET | ProgrammingError | 8.4: HY000, 9.7: HY000 | A loadable function's character-set setter received an invalid argument |
| 3952 | ER_DA_UDF_INVALID_RETURN_TYPE_TO_SET_CHARSET | ProgrammingError | 8.4: HY000, 9.7: HY000 | A loadable function's character-set setter received an invalid return type |
| 3953 | ER_MULTIPLE_INTO_CLAUSES | ProgrammingError | 8.4: HY000, 9.7: HY000 | The SELECT statement contains more than one INTO clause |
| 3954 | ER_MISPLACED_INTO | ProgrammingError | 8.4: HY000, 9.7: HY000 | The SELECT INTO clause appears in an invalid position |
| 3955 | ER_USER_ACCESS_DENIED_FOR_USER_ACCOUNT_BLOCKED_BY_PASSWORD_LOCK | OperationalError | 8.4: HY000, 9.7: HY000 | user access denied for user account blocked by password lock |
| 3956 | ER_WARN_DEPRECATED_YEAR_UNSIGNED | DatabaseError | 8.4: HY000, 9.7: HY000 | UNSIGNED on a YEAR column is deprecated |
| 3957 | ER_CLONE_NETWORK_PACKET | OperationalError | 8.4: HY000, 9.7: HY000 | A clone network packet is invalid or could not be processed |
| 3958 | ER_SDI_OPERATION_FAILED_MISSING_RECORD | OperationalError | 8.4: HY000, 9.7: HY000 | Serialized dictionary information is missing a required record |
| 3959 | ER_DEPENDENT_BY_CHECK_CONSTRAINT | ProgrammingError | 8.4: HY000, 9.7: HY000 | A CHECK constraint depends on the object being changed |
| 3960 | ER_GRP_OPERATION_NOT_ALLOWED_GR_MUST_STOP | OperationalError | 8.4: HY000, 9.7: HY000 | Group replication must be stopped before this operation |
| 3961 | ER_WARN_DEPRECATED_JSON_TABLE_ON_ERROR_ON_EMPTY | DatabaseError | 8.4: HY000, 9.7: HY000 | The JSON_TABLE ordering of ON ERROR before ON EMPTY is deprecated |
| 3962 | ER_WARN_DEPRECATED_INNER_INTO | DatabaseError | 8.4: HY000, 9.7: HY000 | An INTO clause inside this query block is deprecated |
| 3963 | ER_WARN_DEPRECATED_VALUES_FUNCTION_ALWAYS_NULL | DatabaseError | 8.4: HY000, 9.7: HY000 | The VALUES function has no useful value in this context |
| 3964 | ER_WARN_DEPRECATED_SQL_CALC_FOUND_ROWS | DatabaseError | 8.4: HY000, 9.7: HY000 | SQL_CALC_FOUND_ROWS is deprecated |
| 3965 | ER_WARN_DEPRECATED_FOUND_ROWS | DatabaseError | 8.4: HY000, 9.7: HY000 | FOUND_ROWS is deprecated |
| 3966 | ER_MISSING_JSON_VALUE | DataError | 8.4: 22035, 9.7: 22035 | missing JSON value |
| 3967 | ER_MULTIPLE_JSON_VALUES | DataError | 8.4: 22034, 9.7: 22034 | multiple JSON values |
| 3968 | ER_HOSTNAME_TOO_LONG | DataError | 8.4: HY000, 9.7: HY000 | hostname too long |
| 3970 | ER_GROUP_REPLICATION_USER_EMPTY_MSG | ProgrammingError | 8.4: HY000, 9.7: HY000 | The group replication recovery user name is empty |
| 3971 | ER_GROUP_REPLICATION_USER_MANDATORY_MSG | ProgrammingError | 8.4: HY000, 9.7: HY000 | The group replication recovery user name must be supplied |
| 3972 | ER_GROUP_REPLICATION_PASSWORD_LENGTH | DataError | 8.4: HY000, 9.7: HY000 | The group replication recovery password exceeds its supported length |
| 3973 | ER_SUBQUERY_TRANSFORM_REJECTED | NotSupportedError | 8.4: HY000, 9.7: HY000 | subquery transform rejected |
| 3974 | ER_DA_GRP_RPL_RECOVERY_ENDPOINT_FORMAT | ProgrammingError | 8.4: HY000, 9.7: HY000 | The group replication recovery endpoint has an invalid format |
| 3975 | ER_DA_GRP_RPL_RECOVERY_ENDPOINT_INVALID | OperationalError | 8.4: HY000, 9.7: HY000 | The group replication recovery endpoint is invalid |
| 3976 | ER_WRONG_VALUE_FOR_VAR_PLUS_ACTIONABLE_PART | DataError | 8.4: HY000, 9.7: HY000 | The system variable value is invalid; the server message includes corrective details |
| 3977 | ER_STATEMENT_NOT_ALLOWED_AFTER_START_TRANSACTION | ProgrammingError | 8.4: HY000, 9.7: HY000 | statement not allowed after start transaction |
| 3978 | ER_FOREIGN_KEY_WITH_ATOMIC_CREATE_SELECT | NotSupportedError | 8.4: HY000, 9.7: HY000 | A foreign key is incompatible with this atomic CREATE TABLE ... SELECT |
| 3979 | ER_NOT_ALLOWED_WITH_START_TRANSACTION | ProgrammingError | 8.4: HY000, 9.7: HY000 | The requested option is incompatible with START TRANSACTION |
| 3980 | ER_INVALID_JSON_ATTRIBUTE | DataError | 8.4: HY000, 9.7: HY000 | The JSON engine attribute is invalid |
| 3981 | ER_ENGINE_ATTRIBUTE_NOT_SUPPORTED | NotSupportedError | 8.4: HY000, 9.7: HY000 | engine attribute not supported |
| 3982 | ER_INVALID_USER_ATTRIBUTE_JSON | DataError | 8.4: HY000, 9.7: HY000 | The account attribute is not a valid JSON object |
| 3983 | ER_INNODB_REDO_DISABLED | OperationalError | 8.4: HY000, 9.7: HY000 | innodb redo disabled |
| 3984 | ER_INNODB_REDO_ARCHIVING_ENABLED | OperationalError | 8.4: HY000, 9.7: HY000 | innodb redo archiving enabled |
| 3985 | ER_MDL_OUT_OF_RESOURCES | OperationalError | 8.4: HY000, 9.7: HY000 | The server has insufficient resources for a metadata lock |
| 3986 | ER_IMPLICIT_COMPARISON_FOR_JSON | DatabaseError | 8.4: HY000, 9.7: HY000 | The server reports the implicit JSON comparison behavior described in its message |
| 3987 | ER_FUNCTION_DOES_NOT_SUPPORT_CHARACTER_SET | NotSupportedError | 8.4: HY000, 9.7: HY000 | function does not support character set |
| 3988 | ER_IMPOSSIBLE_STRING_CONVERSION | DataError | 8.4: HY000, 9.7: HY000 | impossible string conversion |
| 3989 | ER_SCHEMA_READ_ONLY | OperationalError | 8.4: HY000, 9.7: HY000 | schema read only |
| 3990 | ER_RPL_ASYNC_RECONNECT_GTID_MODE_OFF | OperationalError | 8.4: HY000, 9.7: HY000 | Replication connection failover requires global transaction identifier mode |
| 3991 | ER_RPL_ASYNC_RECONNECT_AUTO_POSITION_OFF | ProgrammingError | 8.4: HY000, 9.7: HY000 | Replication connection failover requires automatic positioning |
| 3992 | ER_DISABLE_GTID_MODE_REQUIRES_ASYNC_RECONNECT_OFF | OperationalError | 8.4: HY000, 9.7: HY000 | Global transaction identifier mode cannot be disabled while replication connection failover is enabled |
| 3993 | ER_DISABLE_AUTO_POSITION_REQUIRES_ASYNC_RECONNECT_OFF | ProgrammingError | 8.4: HY000, 9.7: HY000 | Automatic positioning cannot be disabled while replication connection failover is enabled |
| 3994 | ER_INVALID_PARAMETER_USE | ProgrammingError | 8.4: HY000, 9.7: HY000 | invalid parameter use |
| 3995 | ER_CHARACTER_SET_MISMATCH | ProgrammingError | 8.4: HY000, 9.7: HY000 | character set mismatch |
| 3996 | ER_WARN_VAR_VALUE_CHANGE_NOT_SUPPORTED | NotSupportedError | 8.4: HY000, 9.7: HY000 | Changing the system variable to this value is not supported |
| 3997 | ER_INVALID_TIME_ZONE_INTERVAL | DataError | 8.4: HY000, 9.7: HY000 | invalid time zone interval |
| 3998 | ER_INVALID_CAST | DataError | 8.4: HY000, 9.7: HY000 | invalid cast |
| 3999 | ER_HYPERGRAPH_NOT_SUPPORTED_YET | NotSupportedError | 8.4: 42000, 9.7: 42000 | hypergraph not supported yet |
| 4000 | ER_WARN_HYPERGRAPH_EXPERIMENTAL | DatabaseError | 8.4: HY000 | The hypergraph query optimizer is experimental |
| 4001 | ER_DA_NO_ERROR_LOG_PARSER_CONFIGURED | DatabaseError | 8.4: HY000, 9.7: HY000 | No parser is configured to read the error log |
| 4002 | ER_DA_ERROR_LOG_TABLE_DISABLED | DatabaseError | 8.4: HY000, 9.7: HY000 | The Performance Schema error-log table is disabled |
| 4003 | ER_DA_ERROR_LOG_MULTIPLE_FILTERS | DatabaseError | 8.4: HY000, 9.7: HY000 | The error-log pipeline uses multiple filters whose combined effect requires attention |
| 4004 | ER_DA_CANT_OPEN_ERROR_LOG | OperationalError | 8.4: HY000, 9.7: HY000 | The server could not open the error log |
| 4005 | ER_USER_REFERENCED_AS_DEFINER | DatabaseError | 8.4: HY000, 9.7: HY000 | user referenced as definer |
| 4006 | ER_CANNOT_USER_REFERENCED_AS_DEFINER | ProgrammingError | 8.4: HY000, 9.7: HY000 | The account operation cannot proceed because stored objects use it as their definer |
| 4007 | ER_REGEX_NUMBER_TOO_BIG | DataError | 8.4: HY000, 9.7: HY000 | regex number too big |
| 4008 | ER_SPVAR_NONINTEGER_TYPE | ProgrammingError | 8.4: HY000, 9.7: HY000 | The stored program variable must have an integer type |
| 4009 | WARN_UNSUPPORTED_ACL_TABLES_READ | NotSupportedError | 8.4: HY000, 9.7: HY000 | This access to account privilege tables is not supported |
| 4010 | ER_BINLOG_UNSAFE_ACL_TABLE_READ_IN_DML_DDL | DatabaseError | 8.4: HY000, 9.7: HY000 | Reading privilege tables in this data or schema statement is unsafe for binary logging |
| 4011 | ER_STOP_REPLICA_MONITOR_IO_THREAD_TIMEOUT | OperationalError | 8.4: HY000, 9.7: HY000 | Stopping the replication connection-monitor thread timed out |
| 4012 | ER_STARTING_REPLICA_MONITOR_IO_THREAD | OperationalError | 8.4: HY000, 9.7: HY000 | The replication connection-monitor thread could not be started |
| 4013 | ER_CANT_USE_ANONYMOUS_TO_GTID_WITH_GTID_MODE_NOT_ON | OperationalError | 8.4: HY000, 9.7: HY000 | Assigning identifiers to anonymous transactions requires global transaction identifier mode |
| 4014 | ER_CANT_COMBINE_ANONYMOUS_TO_GTID_AND_AUTOPOSITION | ProgrammingError | 8.4: HY000, 9.7: HY000 | Assigning identifiers to anonymous transactions cannot be combined with auto-positioning |
| 4015 | ER_ASSIGN_GTIDS_TO_ANONYMOUS_TRANSACTIONS_REQUIRES_GTID_MODE_ON | OperationalError | 8.4: HY000, 9.7: HY000 | Global transaction identifier mode must be enabled before anonymous transactions can receive identifiers |
| 4016 | ER_SQL_REPLICA_SKIP_COUNTER_USED_WITH_GTID_MODE_ON | DatabaseError | 8.4: HY000, 9.7: HY000 | The replication skip counter applies only to channels configured to assign identifiers to anonymous transactions |
| 4017 | ER_USING_ASSIGN_GTIDS_TO_ANONYMOUS_TRANSACTIONS_AS_LOCAL_OR_UUID | DatabaseError | 8.4: HY000, 9.7: HY000 | The replica assigns identifiers to anonymous transactions using its local or configured server identifier |
| 4019 | ER_CANT_SET_SQL_AFTER_OR_BEFORE_GTIDS_WITH_ANONYMOUS_TO_GTID | ProgrammingError | 8.4: HY000, 9.7: HY000 | Identifier-based UNTIL conditions conflict with assigning identifiers to anonymous transactions |
| 4020 | ER_ANONYMOUS_TO_GTID_UUID_SAME_AS_GROUP_NAME | ProgrammingError | 8.4: HY000, 9.7: HY000 | The anonymous-transaction identifier conflicts with the replication group identifier |
| 4021 | ER_CANT_USE_SAME_UUID_AS_GROUP_NAME | ProgrammingError | 8.4: HY000, 9.7: HY000 | The server identifier conflicts with the replication group identifier |
| 4022 | ER_GRP_RPL_RECOVERY_CHANNEL_STILL_RUNNING | OperationalError | 8.4: HY000, 9.7: HY000 | The group replication recovery channel is still running |
| 4023 | ER_INNODB_INVALID_AUTOEXTEND_SIZE_VALUE | DataError | 8.4: HY000, 9.7: HY000 | innodb invalid autoextend size value |
| 4024 | ER_INNODB_INCOMPATIBLE_WITH_TABLESPACE | NotSupportedError | 8.4: HY000, 9.7: HY000 | innodb incompatible with tablespace |
| 4025 | ER_INNODB_AUTOEXTEND_SIZE_OUT_OF_RANGE | DataError | 8.4: HY000, 9.7: HY000 | The InnoDB tablespace autoextend size is outside its supported range |
| 4026 | ER_CANNOT_USE_AUTOEXTEND_SIZE_CLAUSE | ProgrammingError | 8.4: HY000, 9.7: HY000 | cannot use autoextend size clause |
| 4027 | ER_ROLE_GRANTED_TO_ITSELF | ProgrammingError | 8.4: HY000, 9.7: HY000 | A role cannot be granted to itself |
| 4028 | ER_TABLE_MUST_HAVE_A_VISIBLE_COLUMN | ProgrammingError | 8.4: HY000, 9.7: HY000 | table must have a visible column |
| 4029 | ER_INNODB_COMPRESSION_FAILURE | OperationalError | 8.4: HY000, 9.7: HY000 | innodb compression failure |
| 4030 | ER_WARN_ASYNC_CONN_FAILOVER_NETWORK_NAMESPACE | DatabaseError | 8.4: HY000, 9.7: HY000 | The server reports the scope of network-namespace support for replication connection failover |
| 4031 | ER_CLIENT_INTERACTION_TIMEOUT | OperationalError | 8.4: HY000, 9.7: HY000 | The server closed the connection after the client remained inactive too long |
| 4032 | ER_INVALID_CAST_TO_GEOMETRY | DataError | 8.4: 22S01, 9.7: 22S01 | invalid cast to geometry |
| 4033 | ER_INVALID_CAST_POLYGON_RING_DIRECTION | DataError | 8.4: 22S04, 9.7: 22S04 | invalid cast polygon ring direction |
| 4034 | ER_GIS_DIFFERENT_SRIDS_AGGREGATION | DataError | 8.4: 22S05, 9.7: 22S05 | A spatial aggregate received geometries with different spatial reference identifiers |
| 4035 | ER_RELOAD_KEYRING_FAILURE | OperationalError | 8.4: HY000, 9.7: HY000 | reload keyring failure |
| 4036 | ER_SDI_GET_KEYS_INVALID_TABLESPACE | NotSupportedError | 8.4: HY000, 9.7: HY000 | Serialized dictionary keys cannot be read from the specified tablespace |
| 4037 | ER_CHANGE_RPL_SRC_WRONG_COMPRESSION_ALGORITHM_SIZE | DataError | 8.4: HY000, 9.7: HY000 | The replication source compression algorithm list exceeds its supported size |
| 4039 | ER_CANT_USE_SAME_UUID_AS_VIEW_CHANGE_UUID | ProgrammingError | 8.4: HY000, 9.7: HY000 | The server identifier conflicts with the group view-change identifier |
| 4040 | ER_ANONYMOUS_TO_GTID_UUID_SAME_AS_VIEW_CHANGE_UUID | ProgrammingError | 8.4: HY000, 9.7: HY000 | The anonymous-transaction identifier conflicts with the group view-change identifier |
| 4041 | ER_GRP_RPL_VIEW_CHANGE_UUID_FAIL_GET_VARIABLE | OperationalError | 8.4: HY000, 9.7: HY000 | The group replication view-change identifier could not be read |
| 4042 | ER_WARN_ADUIT_LOG_MAX_SIZE_AND_PRUNE_SECONDS | DatabaseError | 8.4: HY000, 9.7: HY000 | The audit-log maximum size and pruning age settings have conflicting precedence |
| 4043 | ER_WARN_ADUIT_LOG_MAX_SIZE_CLOSE_TO_ROTATE_ON_SIZE | DataError | 8.4: HY000, 9.7: HY000 | The audit-log size limit is too close to the rotation threshold |
| 4044 | ER_KERBEROS_CREATE_USER | ProgrammingError | 8.4: HY000, 9.7: HY000 | Creating the account for Kerberos authentication failed |
| 4045 | ER_INSTALL_PLUGIN_CONFLICT_CLIENT | NotSupportedError | 8.4: HY000, 9.7: HY000 | The plugin installation conflicts with an existing client-facing plugin |
| 4046 | ER_DA_ERROR_LOG_COMPONENT_FLUSH_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | The error-log component could not be flushed |
| 4047 | ER_WARN_SQL_AFTER_MTS_GAPS_GAP_NOT_CALCULATED | DatabaseError | 8.4: HY000, 9.7: HY000 | The SQL_AFTER_MTS_GAPS condition does not calculate gaps in the current transaction-identifier mode |
| 4048 | ER_INVALID_ASSIGNMENT_TARGET | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid assignment target |
| 4049 | ER_OPERATION_NOT_ALLOWED_ON_GR_SECONDARY | OperationalError | 8.4: HY000, 9.7: HY000 | The operation is not allowed on a group replication secondary |
| 4050 | ER_GRP_RPL_FAILOVER_CHANNEL_STATUS_PROPAGATION | OperationalError | 8.4: HY000, 9.7: HY000 | The group replication failover-channel status could not be propagated |
| 4051 | ER_WARN_AUDIT_LOG_FORMAT_UNIX_TIMESTAMP_ONLY_WHEN_JSON | ProgrammingError | 8.4: HY000, 9.7: HY000 | Unix-timestamp audit-log formatting requires JSON output |
| 4052 | ER_INVALID_MFA_PLUGIN_SPECIFIED | ProgrammingError | 8.4: HY000, 9.7: HY000 | The multi-factor authentication plugin is invalid |
| 4053 | ER_IDENTIFIED_BY_UNSUPPORTED | NotSupportedError | 8.4: HY000, 9.7: HY000 | The authentication plugin does not support IDENTIFIED BY |
| 4054 | ER_INVALID_PLUGIN_FOR_REGISTRATION | NotSupportedError | 8.4: HY000, 9.7: HY000 | The plugin is not valid for authentication registration |
| 4055 | ER_PLUGIN_REQUIRES_REGISTRATION | OperationalError | 8.4: HY000, 9.7: HY000 | The authentication factor requires registration |
| 4056 | ER_MFA_METHOD_EXISTS | ProgrammingError | 8.4: HY000, 9.7: HY000 | The authentication factor already exists |
| 4057 | ER_MFA_METHOD_NOT_EXISTS | ProgrammingError | 8.4: HY000, 9.7: HY000 | The authentication factor does not exist |
| 4058 | ER_AUTHENTICATION_POLICY_MISMATCH | ProgrammingError | 8.4: HY000, 9.7: HY000 | authentication policy mismatch |
| 4059 | ER_PLUGIN_REGISTRATION_DONE | ProgrammingError | 8.4: HY000, 9.7: HY000 | Authentication registration has already been completed |
| 4060 | ER_INVALID_USER_FOR_REGISTRATION | OperationalError | 8.4: HY000, 9.7: HY000 | invalid user for registration |
| 4061 | ER_USER_REGISTRATION_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | user registration failed |
| 4062 | ER_MFA_METHODS_INVALID_ORDER | ProgrammingError | 8.4: HY000, 9.7: HY000 | Authentication factors are specified in an invalid order |
| 4063 | ER_MFA_METHODS_IDENTICAL | ProgrammingError | 8.4: HY000, 9.7: HY000 | Authentication factors must not use the same method |
| 4064 | ER_INVALID_MFA_OPERATIONS_FOR_PASSWORDLESS_USER | ProgrammingError | 8.4: HY000, 9.7: HY000 | The requested multi-factor operation is invalid for a passwordless account |
| 4065 | ER_CHANGE_REPLICATION_SOURCE_NO_OPTIONS_FOR_GTID_ONLY | ProgrammingError | 8.4: HY000, 9.7: HY000 | GTID_ONLY is incompatible with the supplied replication source options |
| 4066 | ER_CHANGE_REP_SOURCE_CANT_DISABLE_REQ_ROW_FORMAT_WITH_GTID_ONLY | ProgrammingError | 8.4: HY000, 9.7: HY000 | REQUIRE_ROW_FORMAT cannot be disabled while GTID_ONLY is enabled |
| 4067 | ER_CHANGE_REP_SOURCE_CANT_DISABLE_AUTO_POSITION_WITH_GTID_ONLY | ProgrammingError | 8.4: HY000, 9.7: HY000 | Automatic positioning cannot be disabled while GTID_ONLY is enabled |
| 4068 | ER_CHANGE_REP_SOURCE_CANT_DISABLE_GTID_ONLY_WITHOUT_POSITIONS | ProgrammingError | 8.4: HY000, 9.7: HY000 | Disabling GTID_ONLY requires explicit replication log coordinates |
| 4069 | ER_CHANGE_REP_SOURCE_CANT_DISABLE_AUTO_POS_WITHOUT_POSITIONS | ProgrammingError | 8.4: HY000, 9.7: HY000 | Disabling automatic positioning requires explicit replication log coordinates |
| 4070 | ER_CHANGE_REP_SOURCE_GR_CHANNEL_WITH_GTID_MODE_NOT_ON | OperationalError | 8.4: HY000, 9.7: HY000 | A group replication channel requires global transaction identifier mode |
| 4071 | ER_CANT_USE_GTID_ONLY_WITH_GTID_MODE_NOT_ON | OperationalError | 8.4: HY000, 9.7: HY000 | GTID_ONLY requires global transaction identifier mode |
| 4072 | ER_WARN_C_DISABLE_GTID_ONLY_WITH_SOURCE_AUTO_POS_INVALID_POS | DatabaseError | 8.4: HY000, 9.7: HY000 | Disabling GTID_ONLY leaves invalid source log coordinates under automatic positioning |
| 4073 | ER_DA_SSL_FIPS_MODE_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | The TLS library could not enter the requested FIPS mode |
| 4074 | ER_VALUE_OUT_OF_RANGE | DataError | 8.4: HY000, 9.7: HY000 | value out of range |
| 4075 | ER_FULLTEXT_WITH_ROLLUP | NotSupportedError | 8.4: HY000, 9.7: HY000 | FULLTEXT search cannot be used with this ROLLUP query |
| 4076 | ER_REGEXP_MISSING_RESOURCE | OperationalError | 8.4: HY000, 9.7: HY000 | regexp missing resource |
| 4077 | ER_WARN_REGEXP_USING_DEFAULT | DatabaseError | 8.4: HY000, 9.7: HY000 | The regular-expression engine is using the default locale |
| 4078 | ER_REGEXP_MISSING_FILE | OperationalError | 8.4: HY000, 9.7: HY000 | regexp missing file |
| 4079 | ER_WARN_DEPRECATED_COLLATION | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated collation |
| 4080 | ER_CONCURRENT_PROCEDURE_USAGE | OperationalError | 8.4: HY000, 9.7: HY000 | concurrent procedure usage |
| 4081 | ER_DA_GLOBAL_CONN_LIMIT | OperationalError | 8.4: HY000, 9.7: HY000 | The global connection limit has been reached |
| 4082 | ER_DA_CONN_LIMIT | OperationalError | 8.4: HY000, 9.7: HY000 | The account connection limit has been reached |
| 4083 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_COLUMN_TYPE_INSTANT | NotSupportedError | 8.4: HY000, 9.7: HY000 | The INSTANT algorithm cannot perform this column type change |
| 4084 | ER_WARN_SF_UDF_NAME_COLLISION | DatabaseError | 8.4: HY000, 9.7: HY000 | A stored function name collides with a loadable function name |
| 4085 | ER_CANNOT_PURGE_BINLOG_WITH_BACKUP_LOCK | OperationalError | 8.4: HY000, 9.7: HY000 | cannot purge binlog with backup lock |
| 4086 | ER_TOO_MANY_WINDOWS | ProgrammingError | 8.4: HY000, 9.7: HY000 | too many windows |
| 4087 | ER_MYSQLBACKUP_CLIENT_MSG | DatabaseError | 8.4: HY000, 9.7: HY000 | The MySQL backup operation reports the condition in the server message |
| 4088 | ER_COMMENT_CONTAINS_INVALID_STRING | DataError | 8.4: HY000, 9.7: HY000 | comment contains invalid string |
| 4089 | ER_DEFINITION_CONTAINS_INVALID_STRING | DataError | 8.4: HY000, 9.7: HY000 | definition contains invalid string |
| 4090 | ER_CANT_EXECUTE_COMMAND_WITH_ASSIGNED_GTID_NEXT | ProgrammingError | 8.4: HY000, 9.7: HY000 | The command cannot run while GTID_NEXT has an explicitly assigned transaction identifier |
| 4091 | ER_XA_TEMP_TABLE | NotSupportedError | 8.4: HY000, 9.7: HY000 | Temporary tables are not allowed in this distributed transaction |
| 4092 | ER_INNODB_MAX_ROW_VERSION | OperationalError | 8.4: HY000, 9.7: HY000 | The InnoDB table has exhausted its instant schema-change row versions |
| 4094 | ER_OPERATION_NOT_ALLOWED_WHILE_PRIMARY_CHANGE_IS_RUNNING | OperationalError | 8.4: HY000, 9.7: HY000 | operation not allowed while primary change is running |
| 4095 | ER_WARN_DEPRECATED_DATETIME_DELIMITER | DatabaseError | 8.4: HY000, 9.7: HY000 | A temporal value uses a deprecated delimiter |
| 4096 | ER_WARN_DEPRECATED_SUPERFLUOUS_DELIMITER | DatabaseError | 8.4: HY000, 9.7: HY000 | A temporal value contains a deprecated redundant delimiter |
| 4097 | ER_CANNOT_PERSIST_SENSITIVE_VARIABLES | OperationalError | 8.4: HY000, 9.7: HY000 | cannot persist sensitive variables |
| 4098 | ER_WARN_CANNOT_SECURELY_PERSIST_SENSITIVE_VARIABLES | DatabaseError | 8.4: HY000, 9.7: HY000 | The sensitive system variable was persisted without encryption |
| 4099 | ER_WARN_TRG_ALREADY_EXISTS | ProgrammingError | 8.4: HY000, 9.7: HY000 | The trigger already exists and IF NOT EXISTS suppressed its creation |
| 4100 | ER_IF_NOT_EXISTS_UNSUPPORTED_TRG_EXISTS_ON_DIFFERENT_TABLE | ProgrammingError | 8.4: HY000, 9.7: HY000 | IF NOT EXISTS cannot suppress a trigger-name conflict on a different table |
| 4101 | ER_IF_NOT_EXISTS_UNSUPPORTED_UDF_NATIVE_FCT_NAME_COLLISION | ProgrammingError | 8.4: HY000, 9.7: HY000 | IF NOT EXISTS cannot suppress a loadable-function name conflict with a built-in function |
| 4102 | ER_SET_PASSWORD_AUTH_PLUGIN_ERROR | NotSupportedError | 8.4: HY000, 9.7: HY000 | The authentication plugin could not change the password |
| 4105 | ER_SRS_INVALID_LATITUDE_OF_ORIGIN | DataError | 8.4: SR002, 9.7: SR002 | spatial reference system invalid latitude of origin |
| 4106 | ER_SRS_INVALID_LONGITUDE_OF_ORIGIN | DataError | 8.4: SR002, 9.7: SR002 | spatial reference system invalid longitude of origin |
| 4107 | ER_SRS_UNUSED_PROJ_PARAMETER_PRESENT | DataError | 8.4: SR002, 9.7: SR002 | The spatial reference system contains an unused projection parameter |
| 4108 | ER_GIPK_COLUMN_EXISTS | ProgrammingError | 8.4: HY000, 9.7: HY000 | The table already contains the reserved generated-primary-key column name |
| 4109 | ER_GIPK_FAILED_AUTOINC_COLUMN_EXISTS | ProgrammingError | 8.4: HY000, 9.7: HY000 | A generated invisible primary key cannot be added because an AUTO_INCREMENT column already exists |
| 4110 | ER_GIPK_COLUMN_ALTER_NOT_ALLOWED | ProgrammingError | 8.4: HY000, 9.7: HY000 | The generated invisible primary key column cannot be altered in this way |
| 4111 | ER_DROP_PK_COLUMN_TO_DROP_GIPK | ProgrammingError | 8.4: HY000, 9.7: HY000 | Dropping the generated invisible primary key requires dropping its column |
| 4112 | ER_CREATE_SELECT_WITH_GIPK_DISALLOWED_IN_SBR | NotSupportedError | 8.4: HY000, 9.7: HY000 | CREATE TABLE ... SELECT with a generated invisible primary key is unsafe for statement-based replication |
| 4114 | ER_CTE_RECURSIVE_NOT_UNION | ProgrammingError | 8.4: HY000, 9.7: HY000 | A recursive common table expression must use the required UNION form |
| 4115 | ER_COMMAND_BACKEND_FAILED_TO_FETCH_SECURITY_CTX | OperationalError | 8.4: HY000, 9.7: HY000 | The internal command service could not obtain its security context |
| 4116 | ER_COMMAND_SERVICE_BACKEND_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | The internal command service backend failed |
| 4117 | ER_CLIENT_FILE_PRIVILEGE_FOR_REPLICATION_CHECKS | OperationalError | 8.4: HY000, 9.7: HY000 | The replication privilege-checking account requires FILE privilege |
| 4118 | ER_GROUP_REPLICATION_FORCE_MEMBERS_COMMAND_FAILURE | OperationalError | 8.4: HY000, 9.7: HY000 | group replication force members command failure |
| 4119 | ER_WARN_DEPRECATED_IDENT | DatabaseError | 8.4: HY000, 9.7: HY000 | The identifier uses a deprecated form |
| 4120 | ER_INTERSECT_ALL_MAX_DUPLICATES_EXCEEDED | DataError | 8.4: HY000, 9.7: HY000 | INTERSECT ALL exceeds the maximum supported duplicate count |
| 4121 | ER_TP_QUERY_THRS_PER_GRP_EXCEEDS_TXN_THR_LIMIT | DataError | 8.4: HY000, 9.7: HY000 | The thread pool's query-thread count exceeds its transaction-thread limit |
| 4122 | ER_BAD_TIMESTAMP_FORMAT | DataError | 8.4: HY000, 9.7: HY000 | bad timestamp format |
| 4123 | ER_SHAPE_PRIDICTION_UDF | DatabaseError | 8.4: HY000, 9.7: HY000 | The SHAPE_PREDICTION loadable function reported the condition in the server message |
| 4124 | ER_SRS_INVALID_HEIGHT | DataError | 8.4: SR002, 9.7: SR002 | spatial reference system invalid height |
| 4125 | ER_SRS_INVALID_SCALING | DataError | 8.4: SR002, 9.7: SR002 | spatial reference system invalid scaling |
| 4126 | ER_SRS_INVALID_ZONE_WIDTH | DataError | 8.4: SR002, 9.7: SR002 | spatial reference system invalid zone width |
| 4127 | ER_SRS_INVALID_LATITUDE_POLAR_STERE_VAR_A | DataError | 8.4: SR002, 9.7: SR002 | The polar stereographic projection has an invalid origin latitude |
| 4128 | ER_WARN_DEPRECATED_CLIENT_NO_SCHEMA_OPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | The client's no-schema connection option is deprecated |
| 4129 | ER_TABLE_NOT_EMPTY | OperationalError | 8.4: HY000, 9.7: HY000 | table not empty |
| 4130 | ER_TABLE_NO_PRIMARY_KEY | ProgrammingError | 8.4: HY000, 9.7: HY000 | table no primary key |
| 4131 | ER_TABLE_IN_SHARED_TABLESPACE | NotSupportedError | 8.4: HY000, 9.7: HY000 | table in shared tablespace |
| 4132 | ER_INDEX_OTHER_THAN_PK | ProgrammingError | 8.4: HY000, 9.7: HY000 | The operation requires a primary index rather than another index type |
| 4133 | ER_LOAD_BULK_DATA_UNSORTED | DataError | 8.4: HY000, 9.7: HY000 | load bulk data unsorted |
| 4134 | ER_BULK_EXECUTOR_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk executor error |
| 4135 | ER_BULK_READER_LIBCURL_INIT_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | The bulk reader could not initialize its network transfer library |
| 4136 | ER_BULK_READER_LIBCURL_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | The bulk reader's network transfer library failed |
| 4137 | ER_BULK_READER_SERVER_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | The bulk reader's remote server reported an error |
| 4138 | ER_BULK_READER_COMMUNICATION_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | bulk reader communication error |
| 4139 | ER_BULK_LOAD_DATA_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | bulk load data failed |
| 4140 | ER_BULK_LOADER_COLUMN_TOO_BIG_FOR_LEFTOVER_BUFFER | DataError | 8.4: HY000, 9.7: HY000 | A bulk-loaded column exceeds the remaining row-buffer capacity |
| 4141 | ER_BULK_LOADER_COMPONENT_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk loader component error |
| 4142 | ER_BULK_LOADER_FILE_CONTAINS_LESS_LINES_THAN_IGNORE_CLAUSE | DataError | 8.4: HY000, 9.7: HY000 | The bulk input has fewer lines than the IGNORE clause requests |
| 4143 | ER_BULK_PARSER_MISSING_ENCLOSED_BY | DataError | 8.4: HY000, 9.7: HY000 | The bulk parser expected an enclosing quote |
| 4144 | ER_BULK_PARSER_ROW_BUFFER_MAX_TOTAL_COLS_EXCEEDED | DataError | 8.4: HY000, 9.7: HY000 | A bulk row buffer exceeds the total permitted column capacity |
| 4145 | ER_BULK_PARSER_COPY_BUFFER_SIZE_EXCEEDED | DataError | 8.4: HY000, 9.7: HY000 | bulk parser copy buffer size exceeded |
| 4146 | ER_BULK_PARSER_UNEXPECTED_END_OF_INPUT | DataError | 8.4: HY000, 9.7: HY000 | bulk parser unexpected end of input |
| 4147 | ER_BULK_PARSER_UNEXPECTED_ROW_TERMINATOR | DataError | 8.4: HY000, 9.7: HY000 | bulk parser unexpected row terminator |
| 4148 | ER_BULK_PARSER_UNEXPECTED_CHAR_AFTER_ENDING_ENCLOSED_BY | DataError | 8.4: HY000, 9.7: HY000 | An unexpected character follows the bulk field's closing quote |
| 4149 | ER_BULK_PARSER_UNEXPECTED_CHAR_AFTER_NULL_ESCAPE | DataError | 8.4: HY000, 9.7: HY000 | bulk parser unexpected char after null escape |
| 4150 | ER_BULK_PARSER_UNEXPECTED_CHAR_AFTER_COLUMN_TERMINATOR | DataError | 8.4: HY000, 9.7: HY000 | An unexpected character follows the bulk column separator |
| 4151 | ER_BULK_PARSER_INCOMPLETE_ESCAPE_SEQUENCE | DataError | 8.4: HY000, 9.7: HY000 | bulk parser incomplete escape sequence |
| 4152 | ER_LOAD_BULK_DATA_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | load bulk data failed |
| 4153 | ER_LOAD_BULK_DATA_WRONG_VALUE_FOR_FIELD | DataError | 8.4: HY000, 9.7: HY000 | load bulk data invalid value for field |
| 4154 | ER_LOAD_BULK_DATA_WARN_NULL_TO_NOTNULL | IntegrityError | 8.4: HY000, 9.7: HY000 | Bulk loading replaced a NULL value for a nonnullable column |
| 4155 | ER_REQUIRE_TABLE_PRIMARY_KEY_CHECK_GENERATE_WITH_GR | ProgrammingError | 8.4: HY000, 9.7: HY000 | Generated-primary-key settings conflict with group replication's primary-key requirement |
| 4156 | ER_CANT_CHANGE_SYS_VAR_IN_READ_ONLY_MODE | OperationalError | 8.4: HY000, 9.7: HY000 | The system variable cannot be changed while the server is read-only |
| 4157 | ER_INNODB_INSTANT_ADD_DROP_NOT_SUPPORTED_MAX_SIZE | NotSupportedError | 8.4: HY000, 9.7: HY000 | The instant column change would exceed InnoDB's maximum row size |
| 4158 | ER_INNODB_INSTANT_ADD_NOT_SUPPORTED_MAX_FIELDS | NotSupportedError | 8.4: HY000, 9.7: HY000 | The instant column addition would exceed InnoDB's maximum column count |
| 4159 | ER_CANT_SET_PERSISTED | OperationalError | 8.4: HY000, 9.7: HY000 | The system variable cannot be persisted |
| 4160 | ER_INSTALL_COMPONENT_SET_NULL_VALUE | DataError | 8.4: HY000, 9.7: HY000 | INSTALL COMPONENT supplies NULL for a required setting |
| 4161 | ER_INSTALL_COMPONENT_SET_UNUSED_VALUE | ProgrammingError | 8.4: HY000, 9.7: HY000 | INSTALL COMPONENT specifies a setting that the component does not use |
| 4162 | ER_WARN_DEPRECATED_USER_DEFINED_COLLATIONS | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated user defined collations |
| 4163 | ER_USER_LOCK_OVERLONG_NAME | ProgrammingError | 8.4: 42000, 9.7: 42000 | user lock overlong name |
| 4164 | ER_WARN_NO_SPACE_VERSION_COMMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | The version comment lacks the required separating space |
| 4165 | ER_VALIDATE_PASSWORD_INSUFFICIENT_CHANGED_CHARACTERS | DataError | 8.4: HY000, 9.7: HY000 | validate password insufficient changed characters |
| 4166 | ER_WARN_DEPRECATED_WITH_NOTE | DatabaseError | 8.4: HY000, 9.7: HY000 | The feature is deprecated; the server message supplies the associated guidance |
| 4167 | ER_CANT_RUN_COMMAND_SERVICES_RECURSIVELY | NotSupportedError | 8.4: HY000, 9.7: HY000 | cannot run command services recursively |
| 4168 | ER_WARN_AUDIT_LOG_FILTER_RECOVERY_LOGGING_DISABLED | DatabaseError | 8.4: HY000, 9.7: HY000 | Audit-log filter recovery disabled logging |
| 4169 | ER_CTE_EXPANSION_LIMIT_EXCEEDED | ProgrammingError | 8.4: HY000, 9.7: HY000 | Expanding the common table expression exceeds the permitted limit |
| 6000 | ER_LANGUAGE_COMPONENT | DatabaseError | 8.4: HY000, 9.7: HY000 | A stored-program language component reported the condition in the server message |
| 6001 | ER_LANGUAGE_COMPONENT_NOT_AVAILABLE | NotSupportedError | 8.4: HY000, 9.7: HY000 | language component not available |
| 6002 | ER_LANGUAGE_COMPONENT_UNSUPPORTED_LANGUAGE | NotSupportedError | 8.4: HY000, 9.7: HY000 | language component unsupported language |
| 6003 | ER_LANGUAGE_COMPONENT_CANNOT_UNINSTALL | OperationalError | 8.4: HY000, 9.7: HY000 | language component cannot uninstall |
| 6004 | ER_SP_NO_ALTER_LANGUAGE | ProgrammingError | 8.4: HY000, 9.7: HY000 | A stored program's implementation language cannot be changed with ALTER |
| 6005 | ER_EXPLAIN_INTO_ANALYZE_NOT_SUPPORTED | NotSupportedError | 8.4: 0A000 | EXPLAIN INTO does not support ANALYZE |
| 6006 | ER_EXPLAIN_INTO_IMPLICIT_FORMAT_NOT_SUPPORTED | NotSupportedError | 8.4: 0A000, 9.7: 0A000 | EXPLAIN INTO requires an explicit supported output format |
| 6007 | ER_EXPLAIN_INTO_FORMAT_NOT_SUPPORTED | NotSupportedError | 8.4: 0A000, 9.7: 0A000 | EXPLAIN INTO does not support the requested format |
| 6008 | ER_NULL_CANT_BE_PERSISTED_FOR_READONLY | NotSupportedError | 8.4: HY000, 9.7: HY000 | NULL cannot be persisted for this read-only system variable |
| 6009 | ER_EXPLAIN_INTO_FOR_CONNECTION_NOT_SUPPORTED | NotSupportedError | 8.4: 0A000, 9.7: 0A000 | EXPLAIN INTO does not support FOR CONNECTION |
| 6010 | ER_INNODB_IMPORT_WRONG_DROPPED_ENUM_LENGTH | DataError | 8.4: HY000, 9.7: HY000 | Imported metadata has an invalid dropped-ENUM definition length |
| 6011 | ER_INNODB_IMPORT_WRONG_NUMBER_OF_INDEXES_ZERO | DataError | 8.4: HY000, 9.7: HY000 | Imported metadata unexpectedly contains no indexes |
| 6012 | ER_INNODB_IMPORT_WRONG_NUMBER_OF_INDEXES_TOO_HIGH | DataError | 8.4: HY000, 9.7: HY000 | Imported metadata contains too many indexes |
| 6013 | ER_INNODB_IMPORT_DROP_COL_METADATA_MISMATCH | DataError | 8.4: HY000, 9.7: HY000 | Imported dropped-column metadata does not match the table definition |
| 6014 | ER_INNODB_IMPORT_ENUM_NULL_TERMINATOR_MISSING | DataError | 8.4: HY000, 9.7: HY000 | An imported ENUM definition lacks its terminating zero byte |
| 6015 | ER_SIMULATED_INJECTION_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | A server fault-injection point produced a simulated failure |
| 6016 | ER_WARN_DEPRECATED_DYNAMIC_PRIV_IN_GRANT | DatabaseError | 8.4: HY000, 9.7: HY000 | This use of a dynamic privilege in GRANT is deprecated |
| 6017 | ER_BULK_MULTI_READER_OPEN_FILE_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | The parallel bulk reader could not open an input file |
| 6018 | ER_BULK_MULTI_READER_READ_FILE_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | The parallel bulk reader could not read an input file |
| 6019 | ER_BULK_MERGE_INVALID_CHUNK | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk merge invalid chunk |
| 6020 | ER_BULK_MERGE_NOT_ALL_CHUNKS_CONSUMED | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk merge not all chunks consumed |
| 6021 | ER_BULK_WRITER_LIBCURL_INIT_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | The bulk writer could not initialize its network transfer library |
| 6022 | ER_BULK_WRITER_LIBCURL_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | The bulk writer's network transfer library failed |
| 6023 | ER_BULK_SORTING_LOADER_WRITE | OperationalError | 8.4: HY000, 9.7: HY000 | Writing sorted bulk-load data failed |
| 6024 | ER_BULK_SORTING_LOADER_WAIT | OperationalError | 8.4: HY000, 9.7: HY000 | Waiting for the sorted bulk-load writer failed |
| 6025 | ER_BULK_READER_OPEN_FILE_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | bulk reader open file failed |
| 6026 | ER_BULK_LOAD_TABLE_HAS_INSTANT_COLS | NotSupportedError | 8.4: HY000, 9.7: HY000 | Bulk loading is not supported for a table with instant-added or instant-dropped columns |
| 6027 | ER_BULK_LOAD_RESOURCE | OperationalError | 8.4: HY000, 9.7: HY000 | The bulk load has insufficient resources |
| 6028 | ER_BULK_LOAD_SECONDARY_ENGINE | NotSupportedError | 8.4: HY000, 9.7: HY000 | The secondary storage engine cannot perform the requested bulk load |
| 6029 | ER_BULK_READER_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk reader error |
| 6030 | ER_BULK_READER_FILE_DOESNT_EXIST | OperationalError | 8.4: HY000, 9.7: HY000 | The bulk input file does not exist |
| 6031 | ER_BULK_READER_COULDNT_RESOLVE_HOST | OperationalError | 8.4: HY000, 9.7: HY000 | The bulk reader could not resolve the remote host name |
| 6032 | ER_START_REPLICA_CHANNEL_INVALID_CONFIGURATION | OperationalError | 8.4: HY000, 9.7: HY000 | start replica channel invalid configuration |
| 6033 | ER_CANNOT_EXECUTE_IN_PRIMARY | NotSupportedError | 8.4: HY000, 9.7: HY000 | cannot execute in primary |
| 6034 | ER_TOO_MANY_GROUP_BY_MODIFIER_BRANCHES | ProgrammingError | 8.4: HY000, 9.7: HY000 | too many group by modifier branches |
| 6035 | ER_WARN_DEPRECATED_ENGINE_SYNTAX_NO_REPLACEMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | The storage engine syntax is deprecated and has no replacement |
| 6036 | ER_QUALIFY_WITHOUT_WINDOW_FUNCTION | ProgrammingError | 8.4: HY000, 9.7: HY000 | QUALIFY requires a window function |
| 6037 | ER_SUPPORTED_ONLY_WITH_HYPERGRAPH | OperationalError | 8.4: HY000, 9.7: HY000 | The operation is supported only by the hypergraph optimizer |
| 6038 | ER_SPECIFIC_ACCESS_DENIED | OperationalError | 8.4: HY000, 9.7: HY000 | The account lacks the specific privilege required by the operation |
| 6039 | ER_CANT_SET_GTID_NEXT_TO_AUTOMATIC_TAGGED_WHEN_GTID_MODE_IS_OFF | ProgrammingError | 8.4: HY000, 9.7: HY000 | GTID_NEXT cannot use the tagged AUTOMATIC form while identifier mode is disabled |
| 6040 | ER_GTID_NEXT_TAG_GTID_MODE_OFF | ProgrammingError | 8.4: HY000, 9.7: HY000 | A tagged transaction identifier is not permitted while identifier mode is disabled |
| 6041 | ER_LH_COL_NOT_NULLABLE | IntegrityError | 8.4: HY000, 9.7: HY000 | Lakehouse column not nullable |
| 6042 | ER_LH_WARN_COL_MISSING_NOT_NULLABLE | IntegrityError | 8.4: HY000, 9.7: HY000 | A missing Lakehouse input column maps to a nonnullable target column |
| 6043 | ER_LH_COL_IS_EMPTY | DataError | 8.4: HY000, 9.7: HY000 | Lakehouse column is empty |
| 6044 | ER_LH_COL_IS_EMPTY_WARN | DatabaseError | 8.4: HY000, 9.7: HY000 | An empty Lakehouse input column was replaced with its default value |
| 6045 | ER_LH_BAD_VALUE | DataError | 8.4: HY000, 9.7: HY000 | Lakehouse bad value |
| 6046 | ER_LH_DECIMAL_UNKNOWN_ERR | DatabaseError | 8.4: HY000, 9.7: HY000 | Converting a Lakehouse decimal value failed |
| 6047 | ER_LH_DECIMAL_OOM_ERR | OperationalError | 8.4: HY000, 9.7: HY000 | Converting a Lakehouse decimal value ran out of memory |
| 6048 | ER_LH_WARN_DECIMAL_ROUNDING | DataError | 8.4: HY000, 9.7: HY000 | A Lakehouse decimal value was rounded |
| 6049 | ER_LH_DECIMAL_PRECISION_EXCEEDS_SCHEMA | DataError | 8.4: HY000, 9.7: HY000 | Lakehouse decimal precision exceeds schema |
| 6050 | ER_LH_EXCEEDS_MIN | DataError | 8.4: HY000, 9.7: HY000 | A Lakehouse value is below the permitted minimum |
| 6051 | ER_LH_EXCEEDS_MAX | DataError | 8.4: HY000, 9.7: HY000 | A Lakehouse value is above the permitted maximum |
| 6052 | ER_LH_WARN_EXCEEDS_MIN_TRUNCATING | DataError | 8.4: HY000, 9.7: HY000 | A Lakehouse value below the minimum was truncated |
| 6053 | ER_LH_WARN_EXCEEDS_MAX_TRUNCATING | DataError | 8.4: HY000, 9.7: HY000 | A Lakehouse value above the maximum was truncated |
| 6054 | ER_LH_REAL_IS_NAN | DataError | 8.4: HY000, 9.7: HY000 | A Lakehouse floating-point value is not a number |
| 6055 | ER_LH_OUT_OF_RANGE | DataError | 8.4: HY000, 9.7: HY000 | Lakehouse out of range |
| 6056 | ER_LH_DATETIME_FORMAT | DataError | 8.4: HY000, 9.7: HY000 | A Lakehouse datetime value has an invalid format |
| 6057 | ER_LH_WARN_TRUNCATED | DataError | 8.4: HY000, 9.7: HY000 | A Lakehouse value was truncated |
| 6058 | ER_LH_CANNOT_CONVERT_STRING | DataError | 8.4: HY000, 9.7: HY000 | Lakehouse cannot convert string |
| 6059 | ER_LH_RESOURCE_PRINCIPAL_ERR | OperationalError | 8.4: HY000, 9.7: HY000 | Lakehouse resource-principal authentication failed |
| 6060 | ER_LH_AWS_AUTH_ERR | OperationalError | 8.4: HY000, 9.7: HY000 | Lakehouse cloud-provider authentication failed |
| 6061 | ER_LH_CSV_PARSING_ERR | DataError | 8.4: HY000, 9.7: HY000 | Parsing a Lakehouse CSV file failed |
| 6062 | ER_LH_COLUMN_MISMATCH_ERR | DataError | 8.4: HY000, 9.7: HY000 | A Lakehouse input record has an incompatible column count |
| 6063 | ER_LH_COLUMN_MAX_ERR | DataError | 8.4: HY000, 9.7: HY000 | A Lakehouse input record exceeds the maximum column count |
| 6064 | ER_LH_CHARSET_UNSUPPORTED | NotSupportedError | 8.4: HY000, 9.7: HY000 | Lakehouse character set unsupported |
| 6065 | ER_LH_PARQUET_DECIMAL_CONVERSION_ERR | DataError | 8.4: HY000, 9.7: HY000 | A Lakehouse Parquet decimal value cannot be converted to the target type |
| 6066 | ER_LH_STRING_TOO_LONG | DataError | 8.4: HY000, 9.7: HY000 | Lakehouse string too long |
| 6067 | ER_LH_RESOURCE_PRINCIPAL_BUCKET_ERR | OperationalError | 8.4: HY000, 9.7: HY000 | Lakehouse resource-principal access to the bucket failed |
| 6068 | ER_LH_NO_FILES_FOUND | OperationalError | 8.4: HY000, 9.7: HY000 | Lakehouse no files found |
| 6069 | ER_LH_EMPTY_FILE | DataError | 8.4: HY000, 9.7: HY000 | Lakehouse empty file |
| 6070 | ER_LH_DUPLICATE_FILE | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse duplicate file |
| 6071 | ER_LH_AVRO_SCHEMA_DEPTH_EXCEEDS_MAX | DataError | 8.4: HY000, 9.7: HY000 | Lakehouse avro schema depth exceeds max |
| 6072 | ER_LH_AVRO_HEADER_MISMATCH | DataError | 8.4: HY000, 9.7: HY000 | Lakehouse avro header mismatch |
| 6073 | ER_LH_AVRO_ENUM_CANNOT_CONVERT_CHARSET | DataError | 8.4: HY000, 9.7: HY000 | A Lakehouse Avro enumeration label cannot be converted to the target character set |
| 6074 | ER_LH_AVRO_ENUM_MISMATCH | DataError | 8.4: HY000, 9.7: HY000 | Lakehouse avro enum mismatch |
| 6075 | ER_LH_AVRO_TYPE_CANNOT_CONVERT | NotSupportedError | 8.4: HY000, 9.7: HY000 | A Lakehouse Avro value cannot be converted to the target type |
| 6076 | ER_LH_AVRO_FILE_ENDS_UNEXPECTEDLY | DataError | 8.4: HY000, 9.7: HY000 | Lakehouse avro file ends unexpectedly |
| 6077 | ER_LH_AVRO_FILE_DATA_CORRUPT | DataError | 8.4: HY000, 9.7: HY000 | Lakehouse avro file data corrupt |
| 6078 | ER_LH_AVRO_INVALID_UNION | NotSupportedError | 8.4: HY000, 9.7: HY000 | Lakehouse avro invalid union |
| 6079 | ER_LH_AVRO_INVALID_BLOCK_SIZE | DataError | 8.4: HY000, 9.7: HY000 | Lakehouse avro invalid block size |
| 6080 | ER_LH_AVRO_INVALID_BLOCK_RECORD_COUNT | DataError | 8.4: HY000, 9.7: HY000 | Lakehouse avro invalid block record count |
| 6081 | ER_LH_FORMAT_HEADER_NO_MAGIC_BYTES | DataError | 8.4: HY000, 9.7: HY000 | The Lakehouse file lacks its format's identifying magic bytes |
| 6082 | ER_LH_AVRO_HEADER_METADATA_ERR | DataError | 8.4: HY000, 9.7: HY000 | The Lakehouse Avro header metadata is invalid |
| 6083 | ER_LH_AVRO_HEADER_NO_SCHEMA | DataError | 8.4: HY000, 9.7: HY000 | The Lakehouse Avro header lacks a schema definition |
| 6084 | ER_LH_AVRO_NO_CODEC_IN_HEADER | DataError | 8.4: HY000, 9.7: HY000 | The Lakehouse Avro header lacks a compression codec |
| 6085 | ER_LH_AVRO_INVALID_NAME_IN_SCHEMA | DataError | 8.4: HY000, 9.7: HY000 | Lakehouse avro invalid name in schema |
| 6086 | ER_LH_AVRO_DECODING_ERR | DataError | 8.4: HY000, 9.7: HY000 | Decoding a Lakehouse Avro value failed |
| 6087 | ER_LH_PARQUET_NON_UTF8_FILE_ENC | NotSupportedError | 8.4: HY000, 9.7: HY000 | The Lakehouse Parquet file uses an unsupported non-UTF-8 encoding |
| 6088 | ER_LH_PARQUET_SCHEMA_MISMATCH | DataError | 8.4: HY000, 9.7: HY000 | Lakehouse parquet schema mismatch |
| 6089 | ER_LH_PARQUET_ROW_GROUP_SIZE_EXCEEDS_MAX | DataError | 8.4: HY000, 9.7: HY000 | Lakehouse parquet row group size exceeds max |
| 6090 | ER_LH_PARQUET_CANNOT_LOCATE_OFFSET | DataError | 8.4: HY000, 9.7: HY000 | The requested offset could not be located in the Lakehouse Parquet file |
| 6091 | ER_LH_PARQUET_TYPE_CANNOT_CONVERT | NotSupportedError | 8.4: HY000, 9.7: HY000 | A Lakehouse Parquet value cannot be converted to the target type |
| 6092 | ER_LH_PARQUET_CANNOT_LOCATE_SCHEMA | DataError | 8.4: HY000, 9.7: HY000 | The required schema could not be located in the Lakehouse Parquet file |
| 6093 | ER_LH_INFER_SCHEMA_MISMATCH / ER_LH_INFER_NUM_COL_MISMATCH | DataError | 8.4: HY000, 9.7: HY000 | Lakehouse schema inference found inconsistent column counts or definitions across inputs |
| 6094 | ER_LH_OOM | OperationalError | 8.4: HY000, 9.7: HY000 | Lakehouse out of memory |
| 6095 | ER_LH_WARN_INFER_SKIPPED_LINES | DataError | 8.4: HY000, 9.7: HY000 | Lakehouse schema inference skipped input lines |
| 6096 | ER_LH_WARN_INFER_SKIPPED_FILES | DataError | 8.4: HY000, 9.7: HY000 | Lakehouse schema inference skipped input files |
| 6097 | ER_LH_INFER_FILE_HAS_NO_DATA | DatabaseError | 8.4: HY000, 9.7: HY000 | The Lakehouse file has no rows usable for schema inference |
| 6098 | ER_LH_INFER_NO_DATA | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse schema inference found no usable data |
| 6099 | ER_LH_INFER_NO_FILES | OperationalError | 8.4: HY000, 9.7: HY000 | Lakehouse schema inference found no matching input files |
| 6100 | ER_LH_WARN_INFER_USE_DEFAULT_COL_NAMES | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse schema inference substituted default column names |
| 6101 | ER_LH_PARQUET_CANNOT_READ_HEADER | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse parquet cannot read header |
| 6102 | ER_LH_INFER_WARN_GOT_EXCEPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse schema inference encountered the exception described by the server message |
| 6103 | ER_LH_AVRO_CANNOT_PARSE_HEADER | DataError | 8.4: HY000, 9.7: HY000 | The Lakehouse Avro header could not be parsed |
| 6104 | ER_LH_PARQUET_CANT_OPEN_FILE | OperationalError | 8.4: HY000, 9.7: HY000 | Lakehouse parquet cannot open file |
| 6105 | ER_LH_TOO_LARGE_VALUE_ERR | DataError | 8.4: HY000, 9.7: HY000 | A Lakehouse value exceeds the supported size |
| 6106 | ER_LH_TOO_LARGE_ROW_ERR | DataError | 8.4: HY000, 9.7: HY000 | A Lakehouse row exceeds the supported size |
| 6107 | ER_TABLESAMPLE_PERCENTAGE | DataError | 8.4: 2202H, 9.7: 2202H | The TABLESAMPLE percentage is invalid |
| 6108 | ER_TABLESAMPLE_ONLY_ON_BASE_TABLES | ProgrammingError | 8.4: HY000, 9.7: HY000 | tablesample only on base tables |
| 6110 | ER_RESULT_SIZE_LIMIT_EXCEEDED | OperationalError | 8.4: HY000, 9.7: HY000 | result size limit exceeded |
| 6111 | ER_LANGUAGE_COMPONENT_INTERNAL | InternalError | 8.4: HY000, 9.7: HY000 | A stored-program language component encountered an internal error |
| 6112 | ER_LANGUAGE_COMPONENT_CONCURRENCY_LIMIT | OperationalError | 8.4: HY000, 9.7: HY000 | A stored-program language component exceeded its concurrency limit |
| 6113 | ER_LANGUAGE_COMPONENT_RUNTIME | DatabaseError | 8.4: HY000, 9.7: HY000 | A stored-program language component encountered a runtime error |
| 6114 | ER_LANGUAGE_COMPONENT_TIMEZONE | NotSupportedError | 8.4: HY000, 9.7: HY000 | A stored-program language component cannot use the specified time zone |
| 6115 | ER_LANGUAGE_COMPONENT_KEYWORD | ProgrammingError | 8.4: HY000, 9.7: HY000 | A stored-program language component received an invalid keyword argument |
| 6116 | ER_LANGUAGE_COMPONENT_SET_SYSTEM_VARIABLE | OperationalError | 8.4: HY000, 9.7: HY000 | A stored-program language component cannot set the system variable |
| 6117 | ER_LANGUAGE_COMPONENT_UNSUPPORTED_TYPE | NotSupportedError | 8.4: HY000, 9.7: HY000 | A stored-program language component does not support the value's type |
| 6118 | ER_LANGUAGE_COMPONENT_CONVERSION | DatabaseError | 8.4: HY000, 9.7: HY000 | A stored-program language component could not convert the value |
| 6119 | ER_WARN_SP_STATEMENT_PARTIALLY_EXECUTED | OperationalError | 8.4: HY000, 9.7: HY000 | The stored program statement was only partially executed |
| 6120 | ER_STMT_EXECUTION_NOT_ALLOWED_WITHIN_SP_OR_TRG_OR_UDF | NotSupportedError | 8.4: HY000, 9.7: HY000 | The statement cannot run inside a stored program, trigger or loadable function |
| 6121 | ER_LH_JSON_PARSING | DataError | 8.4: HY000, 9.7: HY000 | Parsing Lakehouse JSON input failed |
| 6122 | ER_ENGINE_CANNOT_BE_DEFAULT | ProgrammingError | 8.4: HY000, 9.7: HY000 | engine cannot be default |
| 6123 | ER_PARTITION_PREFIX_KEY_NOT_SUPPORTED | NotSupportedError | 8.4: HY000, 9.7: HY000 | partition prefix key not supported |
| 6124 | ER_WARN_DEPRECATED_NON_STANDARD_KEY | DatabaseError | 8.4: HY000, 9.7: HY000 | A foreign key references a nonstandard key definition that is deprecated |
| 6125 | ER_FK_NO_UNIQUE_INDEX_PARENT | ProgrammingError | 8.4: HY000, 9.7: HY000 | The parent table lacks the unique index required by the foreign key |
| 6126 | ER_ACCESS_DENIED_NO_PROXY_GRANT | OperationalError | 8.4: HY000, 9.7: HY000 | Access was denied because the required proxy-account grant is missing |
| 6127 | ER_ACCESS_DENIED_NO_PROXY | OperationalError | 8.4: HY000, 9.7: HY000 | Access was denied because the proxy-account relationship is invalid |
| 6128 | ER_LH_USER_DATA_ACCESS_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | Lakehouse could not access the user's input data |
| 6129 | ER_BULK_READER_ZSTD_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | The bulk reader could not decompress its Zstandard input |
| 6130 | ER_BULK_PARSER_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk parser error |
| 6131 | ER_LH_INVALID_JSON_FILE_FORMAT_SCHEMA | ProgrammingError | 8.4: HY000, 9.7: HY000 | The JSON input's file-format schema is invalid for Lakehouse |
| 6132 | ER_LH_INFER_JSON_INVALID_SCHEMA | DataError | 8.4: HY000, 9.7: HY000 | Lakehouse inferred an invalid JSON schema |
| 6133 | ER_LH_JSON_FILE_FORMAT_WARN_INFER_SCHEMA | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse schema inference adjusted the input to a single JSON column |
| 6134 | ER_NON_SCALAR_USED_AS_KEY | ProgrammingError | 8.4: HY000, 9.7: HY000 | A nonscalar expression cannot be used as an index key |
| 6135 | ER_INCOMPATIBLE_TYPE_AGG | ProgrammingError | 8.4: HY000, 9.7: HY000 | The aggregated expressions have incompatible types |
| 6136 | ER_DATA_INCOMPATIBLE_WITH_VECTOR | DataError | 8.4: HY000, 9.7: HY000 | data incompatible with vector |
| 6137 | ER_EXCEEDS_VECTOR_MAX_DIMENSIONS | DataError | 8.4: HY000, 9.7: HY000 | exceeds vector max dimensions |
| 6138 | ER_TO_VECTOR_CONVERSION | DataError | 8.4: HY000, 9.7: HY000 | A value cannot be converted to a vector |
| 6139 | ER_EXTERNAL_UNSUPPORTED_INDEX_ALGORITHM | DatabaseError | 8.4: HY000 | external unsupported index algorithm |
| 6140 | ER_TP_CANNOT_DISABLE_MTL_WITH_DL | ProgrammingError | 8.4: HY000, 9.7: HY000 | The thread-pool listener options conflict with the configured transaction-thread limit |
| 6141 | ER_GRP_RPL_CONNECTION_HANDOFF_BUSY | OperationalError | 8.4: HY000, 9.7: HY000 | Group replication cannot hand off the connection while it is busy |
| 6400 | ER_SERVER_OFFLINE_MODE_REASON | OperationalError | 9.7: HY000 | The server is in offline mode for the reason given in its message |
| 6402 | ER_LH_UNSTRUCTURED_NO_DATA_FOUND | DatabaseError | 9.7: HY000 | The Lakehouse unstructured input contains no usable data |
| 6403 | ER_LH_UNSTRUCTURED_FILE_PARSE_ERR | DataError | 9.7: HY000 | Parsing a Lakehouse unstructured file failed |
| 6404 | ER_LH_UNSTRUCTURED_ENCODING_ERR | DataError | 9.7: HY000 | A Lakehouse unstructured file has an unsupported or invalid encoding |
| 6405 | ER_LH_UNSTRUCTURED_FILE_TYPE_MISMATCH_ERR | NotSupportedError | 9.7: HY000 | A Lakehouse unstructured file does not match its declared file type |
| 6406 | ER_LH_UNSTRUCTURED_SCHEMA_ERR | ProgrammingError | 9.7: HY000 | A Lakehouse unstructured file has an invalid schema |
| 6407 | ER_LH_COLUMN_ENGINE_ATTR_ERR | ProgrammingError | 9.7: HY000 | A Lakehouse column has an invalid storage-engine attribute |
| 6408 | ER_LH_UNSTRUCTURED_OBJ_READ_ERR | OperationalError | 9.7: HY000 | A Lakehouse unstructured input object could not be read |
| 6409 | ER_LH_NO_SEGMENTS_GENERATED | DatabaseError | 9.7: HY000 | Lakehouse processing produced no usable data segments |
| 6410 | ER_LH_UNSTRUCTURED_FILE_TOO_LARGE | DataError | 9.7: HY000 | Lakehouse unstructured file too large |
| 6411 | ER_EXCEEDED_MAX_REGULAR_STATEMENT_HANDLE_LIMIT | OperationalError | 9.7: HY000 | The server has reached its regular statement-handle limit |
| 6412 | ER_EXPLAIN_ANALYZE_JSON_FORMAT_VERSION_NOT_SUPPORTED | NotSupportedError | 9.7: 0A000 | explain analyze JSON format version not supported |
| 6413 | ER_NON_DML_DYNAMIC_PARAMETERS | ProgrammingError | 9.7: HY000 | Dynamic parameters are not permitted in this non-data-manipulation statement |
| 6414 | ER_DEPRECATE_NON_COMPOSABLE_MULTIPLE_ENGINE | DatabaseError | 9.7: HY000 | Combining these noncomposable storage engines is deprecated |
| 6415 | ER_BULK_JSON_INVALID | DataError | 9.7: HY000 | bulk JSON invalid |
| 6416 | ER_BULK_BLOB_TOO_BIG | DataError | 9.7: HY000 | bulk blob too big |
| 6417 | ER_NO_QUERY_PLAN_FOUND | DatabaseError | 9.7: HY000 | no query plan found |
| 6418 | ER_LH_CP_TOO_MANY_CHANGES | OperationalError | 9.7: HY000 | Lakehouse incremental loading exceeded its change limit and ignored remaining changes |
| 6419 | ER_LH_CP_NOT_ENABLED | OperationalError | 9.7: HY000 | Lakehouse incremental loading is not enabled |
| 6420 | ER_LH_CP_NO_VECTOR_STORE | NotSupportedError | 9.7: HY000 | The Lakehouse vector-store table does not support incremental loading |
| 6421 | ER_LH_CP_NO_VALIDATION_MODE | NotSupportedError | 9.7: HY000 | Lakehouse incremental loading cannot be combined with validation mode |
| 6423 | ER_LH_CP_COMMIT_FAILED | OperationalError | 9.7: HY000 | Committing the Lakehouse incremental load failed |
| 6424 | ER_LH_UNSTRUCTURED_INVALID_LANGUAGE | ProgrammingError | 9.7: HY000 | Lakehouse unstructured invalid language |
| 6425 | ER_LH_VECTOR_TOO_MANY_ENTRIES | DataError | 9.7: HY000 | Lakehouse vector too many entries |
| 6426 | ER_BULK_PARSER_UNEXPECTED_ENCLOSED_BY_CHARACTER | DataError | 9.7: HY000 | The bulk parser encountered an unexpected enclosing quote |
| 6427 | ER_CMD_NEED_SUPER_OR_CREATE_SPATIAL_REFERENCE_SYSTEM | OperationalError | 9.7: HY000 | The operation requires SUPER or CREATE SPATIAL REFERENCE SYSTEM privilege |
| 6428 | ER_CLAUSE_IGNORED | DatabaseError | 9.7: HY000 | clause ignored |
| 6429 | ER_INVALID_CLAUSE_IN_CONTEXT | ProgrammingError | 9.7: HY000 | invalid clause in context |
| 6430 | ER_INVALID_CLAUSE_COMBINATION | ProgrammingError | 9.7: HY000 | invalid clause combination |
| 6431 | ER_WARN_REDACTED_PRIVILEGES | OperationalError | 9.7: HY000 | Privilege names were omitted from the diagnostic because the account cannot inspect them |
| 6432 | ER_INVALID_OUTER_REFERENCE | ProgrammingError | 9.7: HY000 | invalid outer reference |
| 6433 | ER_LIBRARIES_NOT_SUPPORTED | NotSupportedError | 9.7: HY000 | Stored-program libraries are not supported in this configuration |
| 6434 | ER_LH_WARN_SUMMARY | DatabaseError | 9.7: HY000 | The server summarizes the Lakehouse input warnings |
| 6435 | ER_LH_WARN_INFER_USE_DEFAULT_DELIMITERS | DatabaseError | 9.7: HY000 | Lakehouse schema inference selected default field delimiters |
| 6436 | ER_LIBRARY_INVALID | DatabaseError | 9.7: HY000 | library invalid |
| 6437 | ER_CANNOT_PERSIST_TRANSACTION_ISOLATION | NotSupportedError | 9.7: HY000 | cannot persist transaction isolation |
| 6438 | ER_LH_AVRO_INVALID_RECORD | NotSupportedError | 9.7: HY000 | A Lakehouse Avro record is invalid |
| 6439 | ER_LH_INVALID_VALUE_IN_VECTOR | DataError | 9.7: HY000 | Lakehouse invalid value in vector |
| 6440 | ER_LH_WARN_VECTOR_LOSSY_CONVERSION | DataError | 9.7: HY000 | Converting the Lakehouse vector loses precision |
| 6441 | ER_LH_UNSTRUCTURED_TRUNCATION_ERROR | DataError | 9.7: HY000 | Lakehouse unstructured truncation error |
| 6442 | ER_LH_UNSTRUCTURED_PAGE_CHUNK_WARN | DatabaseError | 9.7: HY000 | Lakehouse unstructured input fell back from page chunks to document chunks |
| 6443 | ER_LH_UNSTRUCTURED_TOO_MANY_CHUNKS | DataError | 9.7: HY000 | Lakehouse unstructured too many chunks |
| 6444 | ER_ACCESS_DENIED_FIREWALL | OperationalError | 9.7: 28000 | access denied firewall |
| 6445 | ER_LH_DECOMPRESSION_ERR | DatabaseError | 9.7: HY000 | Decompressing Lakehouse input failed |
| 6446 | ER_JDV_INVALID_DEFINITION_WRONG_ANNOTATIONS | ProgrammingError | 9.7: HY000 | A JSON duality view definition has invalid, duplicate or conflicting annotations |
| 6447 | ER_JDV_ALGO_TEMPTABLE_NOT_SUPPORTED | NotSupportedError | 9.7: HY000 | A JSON duality view cannot use the TEMPTABLE algorithm |
| 6448 | ER_JDV_WITH_CHECK_OPTION_NOT_SUPPORTED | NotSupportedError | 9.7: HY000 | A JSON duality view cannot use WITH CHECK OPTION |
| 6449 | ER_JDV_INVALID_DEFINITION_NON_SIMPLE_SELECT_NOT_SUPPORTED | NotSupportedError | 9.7: HY000 | A JSON duality view requires a supported simple SELECT |
| 6450 | ER_JDV_INVALID_DEFINITION_CTE_NOT_SUPPORTED | NotSupportedError | 9.7: HY000 | A JSON duality view cannot use a common table expression |
| 6451 | ER_JDV_INVALID_DEFINITION_GROUPBY_NOT_SUPPORTED | NotSupportedError | 9.7: HY000 | A JSON duality view cannot use GROUP BY |
| 6452 | ER_JDV_INVALID_DEFINITION_WINDOW_FUNCTION_NOT_SUPPORTED | NotSupportedError | 9.7: HY000 | A JSON duality view cannot use window functions |
| 6453 | ER_JDV_INVALID_DEFINITION_LIMIT_NOT_SUPPORTED | NotSupportedError | 9.7: HY000 | A JSON duality view cannot use LIMIT |
| 6454 | ER_JDV_INVALID_DEFINITION_ORDERBY_NOT_SUPPORTED | NotSupportedError | 9.7: HY000 | A JSON duality view cannot use ORDER BY |
| 6455 | ER_JDV_INVALID_DEFINITION_MULTI_TABLES_NOT_SUPPORTED | NotSupportedError | 9.7: HY000 | This JSON duality view object cannot project multiple tables |
| 6456 | ER_JDV_INVALID_DEFINITION_NO_WHERE_CONDITION_IN_SUBOBJECT | ProgrammingError | 9.7: HY000 | A JSON duality view subobject requires a relationship condition |
| 6457 | ER_JDV_INVALID_DEFINITION_WHERE_CONDITION_IN_ROOTOBJECT | ProgrammingError | 9.7: HY000 | A JSON duality view root object cannot contain a WHERE condition |
| 6458 | ER_JDV_INVALID_DEFINITION_WRONG_WHERE_FORMAT_FOR_SUBOBJECT | ProgrammingError | 9.7: HY000 | A JSON duality view subobject has an invalid relationship condition |
| 6459 | ER_JDV_INVALID_DEFINITION_WRONG_FIELD_TYPE | ProgrammingError | 9.7: HY000 | A JSON duality view field has an invalid type |
| 6460 | ER_JDV_INVALID_DEFINITION_CONTEXT_PREPARE_FAILED | DatabaseError | 9.7: HY000 | Preparing the JSON duality view definition failed |
| 6461 | ER_JDV_INVALID_DEFINITION_NO_JSON_OBJECT_IN_ROOT | ProgrammingError | 9.7: HY000 | A JSON duality view must construct a JSON object at its root |
| 6462 | ER_JDV_INVALID_DEFINITION_MUSTBE_JSON_OBJECT_FOR_SINGLETON | ProgrammingError | 9.7: HY000 | A singleton JSON duality view value must be a JSON object |
| 6463 | ER_JDV_INVALID_DEFINITION_MUSTBE_JSON_OBJECT_FOR_NESTED | ProgrammingError | 9.7: HY000 | A nested JSON duality view value must be a JSON object |
| 6464 | ER_JDV_INVALID_DEFINITION_NO_JSON_OBJ_IN_ARRAYAGG | ProgrammingError | 9.7: HY000 | A JSON duality view array aggregation must contain a JSON object |
| 6465 | ER_JDV_INVALID_DEFINITION_RELATIONSHIP_RULES_VIOLATED | ProgrammingError | 9.7: HY000 | A JSON duality view violates its table-relationship rules |
| 6466 | ER_JDV_INVALID_DEFINITION_COLUMN_TYPE_NOT_SUPPORTED | NotSupportedError | 9.7: HY000 | The column type is unsupported in a JSON duality view |
| 6467 | ER_JDV_INVALID_DEFINITION_NON_BASE_TABLE_NOT_SUPPORTED | ProgrammingError | 9.7: HY000 | A JSON duality view must reference supported base tables |
| 6468 | ER_JDV_INVALID_DEFINITION_TABLE_WITHOUT_PK_FOUND | ProgrammingError | 9.7: HY000 | A table used by a JSON duality view has no primary key |
| 6469 | ER_JDV_INVALID_DEFINITION_TABLE_WITHOUT_PK_PROJECTION_FOUND | ProgrammingError | 9.7: HY000 | A JSON duality view does not project the required primary key columns |
| 6470 | ER_JDV_INVALID_DEFINITION_ID_KEY_USED_BY_NOT_ROOT_TABLE | ProgrammingError | 9.7: HY000 | A nonroot table uses the JSON duality view's reserved identifier key |
| 6471 | ER_JDV_INVALID_DEFINITION_ID_KEY_NOT_USED_BY_ROOT_TABLE | ProgrammingError | 9.7: HY000 | The root table does not supply the JSON duality view's required identifier key |
| 6472 | ER_JDV_INVALID_TABLE_ANNOTATIONS_FOR_SINGLETON_OBJ | ProgrammingError | 9.7: HY000 | A singleton JSON duality view object has invalid table annotations |
| 6473 | ER_JDV_INVALID_TABLE_ANNOTATIONS_FOR_NESTED_OBJ | ProgrammingError | 9.7: HY000 | A nested JSON duality view object has invalid table annotations |
| 6474 | ER_JDV_INVALID_DEFINITION_DUPLICATE_KEYS_NOT_SUPPORTED | ProgrammingError | 9.7: HY000 | A JSON duality view object cannot contain duplicate JSON keys |
| 6475 | ER_JDV_INVALID_DEFINITION_DUPLICATE_COLUMN_NOT_SUPPORTED | ProgrammingError | 9.7: HY000 | A JSON duality view cannot project a column more than once in this context |
| 6476 | ER_JDV_INVALID_DEFINITION_SAME_TABLE_INCONSISTENT_PROJECTION | ProgrammingError | 9.7: HY000 | The same table has inconsistent projections in a JSON duality view |
| 6477 | ER_JDV_INVALID_DEFINITION_COLUMN_LIST_NOT_SUPPORTED | NotSupportedError | 9.7: HY000 | An explicit column list is not supported for the JSON duality view |
| 6478 | ER_JDV_INVALID_DEFINITION_INCONSISTENT_TABLE_FIELD_IN_THE_OBJECT | ProgrammingError | 9.7: HY000 | A JSON duality view object refers inconsistently to its table's fields |
| 6479 | ER_JDV_INVALID_DEFINITION_WHERE_USES_NON_IMMEDIATE_PARENT | ProgrammingError | 9.7: HY000 | A JSON duality view relationship condition must reference the immediate parent |
| 6480 | ER_JDV_INVALID_DEFINITION_COMPOSITE_KEY_USED | NotSupportedError | 9.7: HY000 | This JSON duality view relationship cannot use a composite key |
| 6481 | ER_JDV_INVALID_DEFINITION_ALIAS_NOT_USED_FOR_SAME_TABLES | ProgrammingError | 9.7: HY000 | Repeated table references in a JSON duality view require distinct aliases |
| 6482 | ER_JDV_OPERATION_NOT_SUPPORTED | NotSupportedError | 9.7: 0A000 | The operation is unsupported for a JSON duality view |
| 6483 | ER_JDV_ALTER_OR_REPLACE_NOT_SUPPORTED | NotSupportedError | 9.7: HY000 | A JSON duality view cannot be altered or replaced in this way |
| 6484 | ER_JDV_INVALID_JSON_DUALITY_OBJECT_USAGE | ProgrammingError | 9.7: HY000 | A JSON duality object is used in an invalid context |
| 6485 | ER_JDV_UNEXPECTED_JSON_TYPE | DataError | 9.7: HY000 | A JSON duality view received an unexpected JSON type |
| 6486 | ER_JDV_PRIMARY_KEY_MUST_BE_PROVIDED | IntegrityError | 9.7: HY000 | The JSON duality view operation requires a primary key value |
| 6487 | ER_JDV_JOIN_CONDITION_NOT_SATISFIED | IntegrityError | 9.7: HY000 | The JSON duality view's join condition is not satisfied |
| 6488 | ER_JDV_PK_DUPLICATES_NOT_IDENTICAL | IntegrityError | 9.7: HY000 | Duplicate primary key values in the JSON duality input have inconsistent data |
| 6489 | ER_JDV_UNMAPPED_KEY | DataError | 9.7: HY000 | A JSON duality input key does not map to a view field |
| 6490 | ER_JDV_MISSING_INSERT_TAG | ProgrammingError | 9.7: HY000 | The JSON duality view object lacks permission for insertion |
| 6491 | ER_JDV_MISSING_READONLY_SUBOBJECT | IntegrityError | 9.7: HY000 | A required read-only JSON duality subobject is missing |
| 6492 | ER_JDV_JOIN_CONDITION_VIOLATION | IntegrityError | 9.7: HY000 | The JSON duality input violates a relationship join condition |
| 6493 | ER_JDV_MISSING_ETAG | ProgrammingError | 9.7: HY000 | The JSON duality input lacks the required entity tag |
| 6494 | ER_JDV_ETAG_MISMATCH | OperationalError | 9.7: HY000 | The JSON duality input's entity tag does not match the current data |
| 6495 | ER_JDV_MISSING_VALUE | DataError | 9.7: HY000 | The JSON duality input lacks a required value |
| 6496 | ER_JDV_PK_UPDATES_NOT_ALLOWED | ProgrammingError | 9.7: HY000 | A JSON duality view does not permit primary key updates |
| 6497 | ER_JDV_MISSING_UPDATE_TAG | ProgrammingError | 9.7: HY000 | The JSON duality view object lacks permission for updating |
| 6498 | ER_JDV_MISSING_DELETE_TAG | ProgrammingError | 9.7: HY000 | The JSON duality view object lacks permission for deletion |
| 6499 | ER_JDV_NULL_INSERT_VALUE | DataError | 9.7: HY000 | A JSON duality insert contains NULL where a value is required |
| 6500 | ER_JDV_INSERT_VALUE_NOT_FIXED | ProgrammingError | 9.7: HY000 | A JSON duality insert value cannot be resolved to the required fixed value |
| 6501 | ER_JDV_NULL_UPDATE_VALUE | DataError | 9.7: HY000 | A JSON duality update contains NULL where a value is required |
| 6502 | ER_JDV_UPDATE_VALUE_NOT_FIXED | ProgrammingError | 9.7: HY000 | A JSON duality update value cannot be resolved to the required fixed value |
| 6504 | ER_JDV_CANNOT_BE_USED_WITH | NotSupportedError | 9.7: HY000 | The JSON duality view cannot be used with the specified construct |
| 6505 | ER_JDV_DML_INFO | DatabaseError | 9.7: HY000 | The server reports the JSON duality operation's modification counts |
| 6507 | ER_JDV_INVALID_BINARY_TYPE_HEADER | DataError | 9.7: HY000 | A JSON duality value has an invalid binary type header |
| 6508 | ER_ETAG_NOT_SUPPORTED | NotSupportedError | 9.7: HY000 | Entity tags are not supported in this context |
| 6509 | ER_ENGINE_ATTRIBUTE_CONFLICT | ProgrammingError | 9.7: HY000 | engine attribute conflict |
| 6510 | ER_EXTERNAL_FORMAT_NOT_SUPPORTED | ProgrammingError | 9.7: HY000 | external format not supported |
| 6511 | ER_EXTERNAL_TABLE_INVALID_OPTION | ProgrammingError | 9.7: HY000 | external table invalid option |
| 6512 | ER_EXTERNAL_TABLE_ENGINE_NOT_SUPPORTED | NotSupportedError | 9.7: HY000 | external table engine not supported |
| 6513 | ER_EXTERNAL_TABLE_ENGINE_NOT_SPECIFIED | ProgrammingError | 9.7: HY000 | external table engine not specified |
| 6514 | ER_LH_NO_FILES_FOUND_NO_SECURE_FILE_PRIV | OperationalError | 9.7: HY000 | Lakehouse could not find input files within the permitted import location |
| 6515 | ER_LH_COL_NAME_MISSING | DataError | 9.7: HY000 | Lakehouse column name missing |
| 6516 | ER_LH_COL_NAME_DUPLICATE | DataError | 9.7: HY000 | Lakehouse column name duplicate |
| 6517 | ER_LH_UNSTRUCTURED_OCI_GENAI_FAILURE | OperationalError | 9.7: HY000 | The cloud generative-AI service failed while processing Lakehouse unstructured input |
| 6518 | ER_LH_UNSTRUCTURED_PARTIAL_FILE_PARSE_ERR | DataError | 9.7: HY000 | A Lakehouse unstructured file was only partially parsed |
| 6519 | ER_JDV_EXPLICIT_AUTO_INCREMENT_NOT_ALLOWED | ProgrammingError | 9.7: HY000 | Explicit AUTO_INCREMENT values are not permitted in this JSON duality view operation |
| 6520 | ER_LH_INFER_COL_NAME_MISMATCH | DataError | 9.7: HY000 | Lakehouse schema inference found inconsistent column names |
| 6521 | ER_LH_DELTA_SCHEMA_MISMATCH | DataError | 9.7: HY000 | Lakehouse delta schema mismatch |
| 6522 | ER_LH_DELTA_METADATA_READ_ERROR | DatabaseError | 9.7: HY000 | The Lakehouse Delta metadata could not be read |
| 6523 | ER_LH_DELTA_UNSUPPORTED_ERROR | NotSupportedError | 9.7: HY000 | The Lakehouse Delta input uses an unsupported feature |
| 6524 | ER_WARN_ACCOUNT_TRIMMED | DatabaseError | 9.7: HY000 | Leading or trailing spaces were removed from an account identifier |
| 6525 | ER_LH_WARN_DATETIME_MULTIPLE_PARAM_USE | DatabaseError | 9.7: HY000 | The supplied Lakehouse datetime format options follow the precedence described by the server message |
| 6526 | ER_LH_GENCOL_ERROR | DatabaseError | 9.7: HY000 | Evaluating a Lakehouse generated column failed |
| 6527 | ER_TEMPORAL_FUNCTION_OVERFLOW | DataError | 9.7: 22008 | temporal function overflow |
| 6528 | ER_LH_INFER_COLUMN_MAX_ERR | DataError | 9.7: HY000 | Lakehouse schema inference exceeded its maximum column count |
| 6529 | ER_LH_EXTRA_COLUMNS_IN_FILE | DataError | 9.7: HY000 | Lakehouse extra columns in file |
| 6530 | ER_LH_PARQUET_COMPLEX_CONVERSION | DatabaseError | 9.7: HY000 | A complex Lakehouse Parquet value cannot be converted to the target type |
| 6531 | ER_AUDIT_LOG_DISABLED | OperationalError | 9.7: HY000 | audit log disabled |
| 6532 | ER_AUDIT_LOG_COMPONENT_JSON_FILTER_PARSING_ERROR | DataError | 9.7: HY000 | audit log component JSON filter parsing error |
| 6533 | ER_AUDIT_LOG_FAILED_TO_OPEN_TABLES | OperationalError | 9.7: HY000 | audit log failed to open tables |
| 6534 | ER_AUDIT_LOG_FAILED_TO_OPEN_TABLE | OperationalError | 9.7: HY000 | audit log failed to open table |
| 6535 | ER_AUDIT_LOG_COMPONENT_JSON_FILTER_NAME_CANNOT_BE_EMPTY | ProgrammingError | 9.7: HY000 | audit log component JSON filter name cannot be empty |
| 6536 | ER_AUDIT_LOG_COMPONENT_USER_NAME_INVALID_CHARACTER | ProgrammingError | 9.7: HY000 | audit log component user name invalid character |
| 6537 | ER_AUDIT_LOG_NO_KEYRING_INSTALLED | OperationalError | 9.7: HY000 | audit log no keyring installed |
| 6538 | ER_AUDIT_LOG_COMPONENT_CANNOT_READ_PASSWORD | OperationalError | 9.7: HY000 | audit log component cannot read password |
| 6539 | ER_AUDIT_LOG_COMPONENT_HOST_NAME_INVALID_CHARACTER | ProgrammingError | 9.7: HY000 | audit log component host name invalid character |
| 6540 | ER_AUDIT_LOG_COMPONENT_JSON_USER_NAME_CANNOT_BE_EMPTY | ProgrammingError | 9.7: HY000 | audit log component JSON user name cannot be empty |
| 6541 | ER_AUDIT_LOG_COMPONENT_KEYRING_ID_TIMESTAMP_VALUE_IS_INVALID | DataError | 9.7: HY000 | audit log component keyring id timestamp value is invalid |
| 6542 | ER_AUDIT_LOG_COMPONENT_UDF_INVALID_ARGUMENT_TYPE | ProgrammingError | 9.7: HY000 | An audit-log loadable function received an invalid argument type |
| 6543 | ER_AUDIT_LOG_COMPONENT_UDF_INVALID_ARGUMENT_COUNT | ProgrammingError | 9.7: HY000 | An audit-log loadable function received the wrong argument count |
| 6544 | ER_AUDIT_LOG_ABORT | OperationalError | 9.7: HY000 | audit log abort |
| 6545 | ER_AUDIT_LOG_COULD_NOT_RELOAD_FILTERS | OperationalError | 9.7: HY000 | audit log could not reload filters |
| 6546 | ER_AUDIT_LOG_UDF_INVALID_ARGUMENT_NULL | DataError | 9.7: HY000 | An audit-log loadable function received an unexpected NULL argument |
| 6547 | ER_AUDIT_LOG_MAX_SIZE_CLOSE_TO_ROTATE_ON_SIZE | DataError | 9.7: HY000 | audit log max size close to rotate on size |
| 6548 | ER_AUDIT_LOG_MAX_SIZE_AND_PRUNE_SECONDS_PRECEDENCE | DatabaseError | 9.7: HY000 | Audit-log size and age pruning settings follow the precedence described by the server message |
| 6549 | ER_AUDIT_LOG_PASSWORD_HAS_NOT_BEEN_SET | OperationalError | 9.7: HY000 | audit log password has not been set |
| 6550 | ER_AUDIT_LOG_INVALID_TABLE_DEFINITION | OperationalError | 9.7: HY000 | audit log invalid table definition |
| 6551 | ER_AUDIT_LOG_COMPONENT_FILTER_FAILED_TO_STORE_TABLE_FLDS | OperationalError | 9.7: HY000 | The audit-log filter could not store its table fields |
| 6552 | ER_AUDIT_LOG_COMPONENT_FILTER_FAILED_TO_INIT_TABLE_FOR_READ | OperationalError | 9.7: HY000 | The audit-log filter could not initialize its table for reading |
| 6553 | ER_AUDIT_LOG_COMPONENT_FILTER_FAILED_TO_DELETE_FROM_TABLE | OperationalError | 9.7: HY000 | audit log component filter failed to delete from table |
| 6554 | ER_AUDIT_LOG_JSON_FILTER_DOES_NOT_EXIST_USER | ProgrammingError | 9.7: HY000 | The specified audit-log JSON filter does not exist for the account |
| 6555 | ER_LH_CUSTOM_DATA_PLACEMENT_OVERRIDES | DatabaseError | 9.7: HY000 | Lakehouse custom data placement overrides the default placement |
| 6556 | ER_LH_AVRO_FILES_DATA_TYPE_MISMATCH | DataError | 9.7: HY000 | Lakehouse avro files data type mismatch |
| 6557 | ER_AUDIT_LOG_COMPONENT_INVALID_FILTER_ABORT | OperationalError | 9.7: HY000 | An invalid audit-log filter caused the operation to abort |
| 6558 | ER_AUDIT_LOG_COMPONENT_JSON_INVALID_FILTER_CANNOT_BE_USED | OperationalError | 9.7: HY000 | The invalid audit-log JSON filter cannot be used |
| 6559 | ER_WARN_AUDIT_LOG_FILTERS_INVALID | DatabaseError | 9.7: HY000 | warn audit log filters invalid |
| 6560 | ER_JDV_GENERATED_AND_EXPLICIT_PKS_NOT_ALLOWED | ProgrammingError | 9.7: HY000 | A JSON duality operation cannot combine generated and explicit primary keys |
| 6561 | ER_MASKING_POLICY_COMPONENT_ERROR | DatabaseError | 9.7: HY000 | masking policy component error |
| 6562 | ER_MASKING_POLICY_ALREADY_EXISTS | ProgrammingError | 9.7: 42000 | masking policy already exists |
| 6563 | ER_MASKING_POLICY_EXPECTS_CASE | ProgrammingError | 9.7: 42000 | A masking policy requires a CASE expression |
| 6564 | ER_MASKING_POLICY_EXPECTS_CASE_WHEN | ProgrammingError | 9.7: 42000 | A masking policy requires a CASE WHEN expression |
| 6565 | ER_MASKING_POLICY_MISSING_ELSE | ProgrammingError | 9.7: 42000 | A masking policy requires an ELSE branch |
| 6566 | ER_MASKING_POLICY_ONE_WHEN_REQUIRED | ProgrammingError | 9.7: 42000 | A masking policy requires exactly one WHEN branch |
| 6567 | ER_MASKING_POLICY_MUST_RETURN_ARGUMENT | ProgrammingError | 9.7: 42000 | A masking policy must return its input argument in the required branch |
| 6568 | ER_MASKING_POLICY_INVALID_GATEKEEPER | ProgrammingError | 9.7: 42000 | A masking policy uses an unsupported gatekeeper function |
| 6569 | ER_MASKING_POLICY_NON_LITERAL_GATEKEEPER_ARG | ProgrammingError | 9.7: 42000 | A masking policy gatekeeper argument must be a literal |
| 6570 | ER_MASKING_POLICY_DISALLOWED_CONSTRUCT | ProgrammingError | 9.7: 42000 | masking policy disallowed construct |
| 6571 | ER_MASKING_POLICY_INVALID_COLUMN_REFERENCE | ProgrammingError | 9.7: 42000 | masking policy invalid column reference |
| 6572 | ER_MASKING_POLICY_INCOMPATIBLE_COLUMN_FEATURE | ProgrammingError | 9.7: 42000 | masking policy incompatible column feature |
| 6573 | ER_MASKING_POLICY_INCOMPATIBLE_TYPES | ProgrammingError | 9.7: 42000 | masking policy incompatible types |
| 6574 | ER_MASKING_POLICY_NON_DETERMINISTIC_FUNC | ProgrammingError | 9.7: 42000 | A masking policy cannot use a nondeterministic function |
| 6575 | ER_FK_MAX_TABLES_IN_CASCADE_CHAIN_EXCEEDED | IntegrityError | 9.7: HY000 | The foreign key cascade chain exceeds the maximum table count |
| 6576 | ER_FK_CASCADE_TRIGGER_UPDATING_FK_COLUMNS_NOT_SUPPORTED | NotSupportedError | 9.7: HY000 | A cascade trigger cannot update the affected foreign key columns |
| 6577 | ER_CANT_UPDATE_USED_TABLE_IN_FK_CASCADE | ProgrammingError | 9.7: HY000 | A foreign key cascade cannot modify a table already in use by the invoking statement |
| 6578 | ER_WARN_SET_OPERATIONS_BUFFER | OperationalError | 9.7: HY000 | The set-operation buffer is too small for the hashed operation; optimizer tracing supplies recommendations |
| 6579 | ER_JDV_UNEXPECTED_INVALID_VALUE | InternalError | 9.7: HY000 | A JSON duality operation encountered an invalid value |

## Frozen sources

- {"kind": "official installed client messages", "path": "/opt/homebrew/opt/mysql@8.4/share/mysql/messages_to_clients.txt", "sha256": "8e081588313b9f711f186c16ce21961a182c0dfe343f26c1039a2e5d45e7404b"}
- {"kind": "official installed generated header", "path": "/opt/homebrew/opt/mysql@8.4/include/mysql/mysqld_error.h", "sha256": "bd87e9b79575a335473bed6e782a8b920bb45781145f10fa7b1f4f4822337409"}
- {"kind": "actual dedicated-server registration", "path": "/Users/jef/ResilioSync/code/SQLAero/evidence/rust-sync-kernel/startup/error-catalog-84-observed.json", "sha256": "3fe61eacfc0d2b3af3168bd6da73868c2647297b341399a22a688f6cc83e3470"}
- {"kind": "official shared global error reference", "url": "https://dev.mysql.com/doc/mysql-errors/8.4/en/global-error-reference.html", "path": "/Users/jef/ResilioSync/code/SQLAero/evidence/rust-sync-kernel/startup/global-errors-84.html", "sha256": "88c1c7fbe7181b40152a343773746346b05ecdd94d8a852450465b06ecd20a26"}
- {"kind": "official installed client messages", "path": "/opt/homebrew/opt/mysql@9.7/share/mysql/messages_to_clients.txt", "sha256": "0eeede7d65afc21e57955a530b653a30bfe87ddecf3b36d0f33bf9726cffa211"}
- {"kind": "official installed generated header", "path": "/opt/homebrew/opt/mysql@9.7/include/mysql/mysqld_error.h", "sha256": "2971e130202f870c41fd750bd08960a03f64ccf8d47a4ce06cd789a156ef1b59"}
- {"kind": "actual dedicated-server registration", "path": "/Users/jef/ResilioSync/code/SQLAero/evidence/rust-sync-kernel/startup/error-catalog-97-observed.json", "sha256": "2ffe44137d6e65e735f72c13e94e907048ac2dfca48ad5ef73109448b17f25c2"}
- {"kind": "official shared global error reference", "url": "https://dev.mysql.com/doc/mysql-errors/9.7/en/global-error-reference.html", "path": "/Users/jef/ResilioSync/code/SQLAero/evidence/rust-sync-kernel/startup/global-errors-97.html", "sha256": "59b8731b16d3912f2061a949acae65573d7650f3c131fba6d0f691d93421ec23"}
