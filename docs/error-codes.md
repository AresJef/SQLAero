# SQLAero error code registry

Generated from data/error-catalog.json (SHA-256 a29fb8e7774ce544cff1f905eaec98d8b5369db5c047484c04c56836172872aa); edit the registry and run tools/error_catalog.py.

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
| 1 | EE_CANTCREATEFILE | OperationalError | 8.4: HY000, 9.7: HY000 | cantcreatefile |
| 2 | EE_READ | OperationalError | 8.4: HY000, 9.7: HY000 | read |
| 3 | EE_WRITE | OperationalError | 8.4: HY000, 9.7: HY000 | write |
| 4 | EE_BADCLOSE | OperationalError | 8.4: HY000, 9.7: HY000 | badclose |
| 5 | EE_OUTOFMEMORY | OperationalError | 8.4: HY000, 9.7: HY000 | out of memory |
| 6 | EE_DELETE | OperationalError | 8.4: HY000, 9.7: HY000 | delete |
| 7 | EE_LINK | OperationalError | 8.4: HY000, 9.7: HY000 | link |
| 9 | EE_EOFERR | OperationalError | 8.4: HY000, 9.7: HY000 | eoferr |
| 10 | EE_CANTLOCK | OperationalError | 8.4: HY000, 9.7: HY000 | cantlock |
| 11 | EE_CANTUNLOCK | OperationalError | 8.4: HY000, 9.7: HY000 | cantunlock |
| 12 | EE_DIR | OperationalError | 8.4: HY000, 9.7: HY000 | dir |
| 13 | EE_STAT | OperationalError | 8.4: HY000, 9.7: HY000 | stat |
| 14 | EE_CANT_CHSIZE | OperationalError | 8.4: HY000, 9.7: HY000 | cannot chsize |
| 15 | EE_CANT_OPEN_STREAM | OperationalError | 8.4: HY000, 9.7: HY000 | cannot open stream |
| 16 | EE_GETWD | OperationalError | 8.4: HY000, 9.7: HY000 | getwd |
| 17 | EE_SETWD | OperationalError | 8.4: HY000, 9.7: HY000 | setwd |
| 18 | EE_LINK_WARNING | OperationalError | 8.4: HY000, 9.7: HY000 | link warning |
| 19 | EE_OPEN_WARNING | OperationalError | 8.4: HY000, 9.7: HY000 | open warning |
| 20 | EE_DISK_FULL | OperationalError | 8.4: HY000, 9.7: HY000 | disk full |
| 21 | EE_CANT_MKDIR | OperationalError | 8.4: HY000, 9.7: HY000 | cannot mkdir |
| 22 | EE_UNKNOWN_CHARSET | NotSupportedError | 8.4: HY000, 9.7: HY000 | unknown character set |
| 23 | EE_OUT_OF_FILERESOURCES | OperationalError | 8.4: HY000, 9.7: HY000 | out of fileresources |
| 24 | EE_CANT_READLINK | OperationalError | 8.4: HY000, 9.7: HY000 | cannot readlink |
| 25 | EE_CANT_SYMLINK | OperationalError | 8.4: HY000, 9.7: HY000 | cannot symlink |
| 26 | EE_REALPATH | OperationalError | 8.4: HY000, 9.7: HY000 | realpath |
| 27 | EE_SYNC | OperationalError | 8.4: HY000, 9.7: HY000 | sync |
| 28 | EE_UNKNOWN_COLLATION | NotSupportedError | 8.4: HY000, 9.7: HY000 | unknown collation |
| 29 | EE_FILENOTFOUND | OperationalError | 8.4: HY000, 9.7: HY000 | filenotfound |
| 30 | EE_FILE_NOT_CLOSED | OperationalError | 8.4: HY000, 9.7: HY000 | file not closed |
| 31 | EE_CHANGE_OWNERSHIP | OperationalError | 8.4: HY000, 9.7: HY000 | change ownership |
| 32 | EE_CHANGE_PERMISSIONS | OperationalError | 8.4: HY000, 9.7: HY000 | change permissions |
| 33 | EE_CANT_SEEK | OperationalError | 8.4: HY000, 9.7: HY000 | cannot seek |
| 34 | EE_CAPACITY_EXCEEDED | OperationalError | 8.4: HY000, 9.7: HY000 | capacity exceeded |
| 35 | EE_DISK_FULL_WITH_RETRY_MSG | OperationalError | 8.4: HY000, 9.7: HY000 | disk full with retry msg |
| 36 | EE_FAILED_TO_CREATE_TIMER | OperationalError | 8.4: HY000, 9.7: HY000 | failed to create timer |
| 37 | EE_FAILED_TO_DELETE_TIMER | OperationalError | 8.4: HY000, 9.7: HY000 | failed to delete timer |
| 38 | EE_FAILED_TO_CREATE_TIMER_QUEUE | OperationalError | 8.4: HY000, 9.7: HY000 | failed to create timer queue |
| 39 | EE_FAILED_TO_START_TIMER_NOTIFY_THREAD | OperationalError | 8.4: HY000, 9.7: HY000 | failed to start timer notify thread |
| 40 | EE_FAILED_TO_CREATE_TIMER_NOTIFY_THREAD_INTERRUPT_EVENT | OperationalError | 8.4: HY000, 9.7: HY000 | failed to create timer notify thread interrupt event |
| 41 | EE_EXITING_TIMER_NOTIFY_THREAD | OperationalError | 8.4: HY000, 9.7: HY000 | exiting timer notify thread |
| 42 | EE_WIN_LIBRARY_LOAD_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | win library load failed |
| 43 | EE_WIN_RUN_TIME_ERROR_CHECK | OperationalError | 8.4: HY000, 9.7: HY000 | win run time error check |
| 44 | EE_FAILED_TO_DETERMINE_LARGE_PAGE_SIZE | OperationalError | 8.4: HY000, 9.7: HY000 | failed to determine large page size |
| 45 | EE_FAILED_TO_KILL_ALL_THREADS | OperationalError | 8.4: HY000, 9.7: HY000 | failed to kill all threads |
| 46 | EE_FAILED_TO_CREATE_IO_COMPLETION_PORT | OperationalError | 8.4: HY000, 9.7: HY000 | failed to create I/O completion port |
| 47 | EE_FAILED_TO_OPEN_DEFAULTS_FILE | OperationalError | 8.4: HY000, 9.7: HY000 | failed to open defaults file |
| 48 | EE_FAILED_TO_HANDLE_DEFAULTS_FILE | OperationalError | 8.4: HY000, 9.7: HY000 | failed to handle defaults file |
| 49 | EE_WRONG_DIRECTIVE_IN_CONFIG_FILE | OperationalError | 8.4: HY000, 9.7: HY000 | invalid directive in config file |
| 50 | EE_SKIPPING_DIRECTIVE_DUE_TO_MAX_INCLUDE_RECURSION | OperationalError | 8.4: HY000, 9.7: HY000 | skipping directive due to max include recursion |
| 51 | EE_INCORRECT_GRP_DEFINITION_IN_CONFIG_FILE | OperationalError | 8.4: HY000, 9.7: HY000 | incorrect grp definition in config file |
| 52 | EE_OPTION_WITHOUT_GRP_IN_CONFIG_FILE | OperationalError | 8.4: HY000, 9.7: HY000 | option without grp in config file |
| 53 | EE_CONFIG_FILE_PERMISSION_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | config file permission error |
| 54 | EE_IGNORE_WORLD_WRITABLE_CONFIG_FILE | OperationalError | 8.4: HY000, 9.7: HY000 | ignore world writable config file |
| 55 | EE_USING_DISABLED_OPTION | OperationalError | 8.4: HY000, 9.7: HY000 | using disabled option |
| 56 | EE_USING_DISABLED_SHORT_OPTION | OperationalError | 8.4: HY000, 9.7: HY000 | using disabled short option |
| 57 | EE_USING_PASSWORD_ON_CLI_IS_INSECURE | OperationalError | 8.4: HY000, 9.7: HY000 | using password on cli is insecure |
| 58 | EE_UNKNOWN_SUFFIX_FOR_VARIABLE | OperationalError | 8.4: HY000, 9.7: HY000 | unknown suffix for variable |
| 59 | EE_SSL_ERROR_FROM_FILE | OperationalError | 8.4: HY000, 9.7: HY000 | SSL error from file |
| 60 | EE_SSL_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | SSL error |
| 61 | EE_NET_SEND_ERROR_IN_BOOTSTRAP | OperationalError | 8.4: HY000, 9.7: HY000 | net send error in bootstrap |
| 62 | EE_PACKETS_OUT_OF_ORDER | OperationalError | 8.4: HY000, 9.7: HY000 | packets out of order |
| 63 | EE_UNKNOWN_PROTOCOL_OPTION | OperationalError | 8.4: HY000, 9.7: HY000 | unknown protocol option |
| 64 | EE_FAILED_TO_LOCATE_SERVER_PUBLIC_KEY | OperationalError | 8.4: HY000, 9.7: HY000 | failed to locate server public key |
| 65 | EE_PUBLIC_KEY_NOT_IN_PEM_FORMAT | OperationalError | 8.4: HY000, 9.7: HY000 | public key not in pem format |
| 66 | EE_DEBUG_INFO | OperationalError | 8.4: HY000, 9.7: HY000 | debug info |
| 67 | EE_UNKNOWN_VARIABLE | OperationalError | 8.4: HY000, 9.7: HY000 | unknown variable |
| 68 | EE_UNKNOWN_OPTION | OperationalError | 8.4: HY000, 9.7: HY000 | unknown option |
| 69 | EE_UNKNOWN_SHORT_OPTION | OperationalError | 8.4: HY000, 9.7: HY000 | unknown short option |
| 70 | EE_OPTION_WITHOUT_ARGUMENT | OperationalError | 8.4: HY000, 9.7: HY000 | option without argument |
| 71 | EE_OPTION_REQUIRES_ARGUMENT | OperationalError | 8.4: HY000, 9.7: HY000 | option requires argument |
| 72 | EE_SHORT_OPTION_REQUIRES_ARGUMENT | OperationalError | 8.4: HY000, 9.7: HY000 | short option requires argument |
| 73 | EE_OPTION_IGNORED_DUE_TO_INVALID_VALUE | OperationalError | 8.4: HY000, 9.7: HY000 | option ignored due to invalid value |
| 74 | EE_OPTION_WITH_EMPTY_VALUE | OperationalError | 8.4: HY000, 9.7: HY000 | option with empty value |
| 75 | EE_FAILED_TO_ASSIGN_MAX_VALUE_TO_OPTION | OperationalError | 8.4: HY000, 9.7: HY000 | failed to assign max value to option |
| 76 | EE_INCORRECT_BOOLEAN_VALUE_FOR_OPTION | OperationalError | 8.4: HY000, 9.7: HY000 | incorrect boolean value for option |
| 77 | EE_FAILED_TO_SET_OPTION_VALUE | OperationalError | 8.4: HY000, 9.7: HY000 | failed to set option value |
| 78 | EE_INCORRECT_INT_VALUE_FOR_OPTION | OperationalError | 8.4: HY000, 9.7: HY000 | incorrect int value for option |
| 79 | EE_INCORRECT_UINT_VALUE_FOR_OPTION | OperationalError | 8.4: HY000, 9.7: HY000 | incorrect uint value for option |
| 80 | EE_ADJUSTED_SIGNED_VALUE_FOR_OPTION | OperationalError | 8.4: HY000, 9.7: HY000 | adjusted signed value for option |
| 81 | EE_ADJUSTED_UNSIGNED_VALUE_FOR_OPTION | OperationalError | 8.4: HY000, 9.7: HY000 | adjusted unsigned value for option |
| 82 | EE_ADJUSTED_ULONGLONG_VALUE_FOR_OPTION | OperationalError | 8.4: HY000, 9.7: HY000 | adjusted ulonglong value for option |
| 83 | EE_ADJUSTED_DOUBLE_VALUE_FOR_OPTION | OperationalError | 8.4: HY000, 9.7: HY000 | adjusted double value for option |
| 84 | EE_INVALID_DECIMAL_VALUE_FOR_OPTION | OperationalError | 8.4: HY000, 9.7: HY000 | invalid decimal value for option |
| 85 | EE_COLLATION_PARSER_ERROR | OperationalError | 8.4: HY000, 9.7: HY000 | collation parser error |
| 86 | EE_FAILED_TO_RESET_BEFORE_PRIMARY_IGNORABLE_CHAR | OperationalError | 8.4: HY000, 9.7: HY000 | failed to reset before primary ignorable char |
| 87 | EE_FAILED_TO_RESET_BEFORE_TERTIARY_IGNORABLE_CHAR | OperationalError | 8.4: HY000, 9.7: HY000 | failed to reset before tertiary ignorable char |
| 88 | EE_SHIFT_CHAR_OUT_OF_RANGE | OperationalError | 8.4: HY000, 9.7: HY000 | shift char out of range |
| 89 | EE_RESET_CHAR_OUT_OF_RANGE | OperationalError | 8.4: HY000, 9.7: HY000 | reset char out of range |
| 90 | EE_UNKNOWN_LDML_TAG | OperationalError | 8.4: HY000, 9.7: HY000 | unknown ldml tag |
| 91 | EE_FAILED_TO_RESET_BEFORE_SECONDARY_IGNORABLE_CHAR | OperationalError | 8.4: HY000, 9.7: HY000 | failed to reset before secondary ignorable char |
| 92 | EE_FAILED_PROCESSING_DIRECTIVE | OperationalError | 8.4: HY000, 9.7: HY000 | failed processing directive |
| 93 | EE_PTHREAD_KILL_FAILED | OperationalError | 8.4: HY000, 9.7: HY000 | POSIX thread kill failed |
| 94 | EE_COLLATION_ALIAS_ERROR | NotSupportedError | 9.7: HY000 | collation alias error |
| 1004 | ER_CANT_CREATE_FILE | OperationalError | 8.4: HY000, 9.7: HY000 | cannot create file |
| 1005 | ER_CANT_CREATE_TABLE | OperationalError | 8.4: HY000, 9.7: HY000 | cannot create table |
| 1006 | ER_CANT_CREATE_DB | OperationalError | 8.4: HY000, 9.7: HY000 | cannot create database |
| 1007 | ER_DB_CREATE_EXISTS | DatabaseError | 8.4: HY000, 9.7: HY000 | database create exists |
| 1008 | ER_DB_DROP_EXISTS | DatabaseError | 8.4: HY000, 9.7: HY000 | database drop exists |
| 1010 | ER_DB_DROP_RMDIR | DatabaseError | 8.4: HY000, 9.7: HY000 | database drop rmdir |
| 1012 | ER_CANT_FIND_SYSTEM_REC | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot find system rec |
| 1013 | ER_CANT_GET_STAT | OperationalError | 8.4: HY000, 9.7: HY000 | cannot get stat |
| 1015 | ER_CANT_LOCK | OperationalError | 8.4: HY000, 9.7: HY000 | cannot lock |
| 1016 | ER_CANT_OPEN_FILE | OperationalError | 8.4: HY000, 9.7: HY000 | cannot open file |
| 1017 | ER_FILE_NOT_FOUND | OperationalError | 8.4: HY000, 9.7: HY000 | file not found |
| 1018 | ER_CANT_READ_DIR | OperationalError | 8.4: HY000, 9.7: HY000 | cannot read dir |
| 1020 | ER_CHECKREAD | DatabaseError | 8.4: HY000, 9.7: HY000 | checkread |
| 1022 | ER_DUP_KEY | IntegrityError | 8.4: 23000, 9.7: 23000 | dup key |
| 1024 | ER_ERROR_ON_READ | OperationalError | 8.4: HY000, 9.7: HY000 | error on read |
| 1025 | ER_ERROR_ON_RENAME | DatabaseError | 8.4: HY000, 9.7: HY000 | error on rename |
| 1026 | ER_ERROR_ON_WRITE | OperationalError | 8.4: HY000, 9.7: HY000 | error on write |
| 1027 | ER_FILE_USED | DatabaseError | 8.4: HY000, 9.7: HY000 | file used |
| 1030 | ER_GET_ERRNO | DatabaseError | 8.4: HY000, 9.7: HY000 | get errno |
| 1031 | ER_ILLEGAL_HA | DatabaseError | 8.4: HY000, 9.7: HY000 | illegal ha |
| 1032 | ER_KEY_NOT_FOUND | DatabaseError | 8.4: HY000, 9.7: HY000 | key not found |
| 1033 | ER_NOT_FORM_FILE | DatabaseError | 8.4: HY000, 9.7: HY000 | not form file |
| 1034 | ER_NOT_KEYFILE | InternalError | 8.4: HY000, 9.7: HY000 | not keyfile |
| 1035 | ER_OLD_KEYFILE | DatabaseError | 8.4: HY000, 9.7: HY000 | old keyfile |
| 1036 | ER_OPEN_AS_READONLY | DatabaseError | 8.4: HY000, 9.7: HY000 | open as readonly |
| 1037 | ER_OUTOFMEMORY | OperationalError | 8.4: HY001, 9.7: HY001 | out of memory |
| 1038 | ER_OUT_OF_SORTMEMORY | DatabaseError | 8.4: HY001, 9.7: HY001 | out of sortmemory |
| 1040 | ER_CON_COUNT_ERROR | OperationalError | 8.4: 08004, 9.7: 08004 | con count error |
| 1041 | ER_OUT_OF_RESOURCES | OperationalError | 8.4: HY000, 9.7: HY000 | out of resources |
| 1042 | ER_BAD_HOST_ERROR | OperationalError | 8.4: 08S01, 9.7: 08S01 | bad host error |
| 1043 | ER_HANDSHAKE_ERROR | OperationalError | 8.4: 08S01, 9.7: 08S01 | handshake error |
| 1044 | ER_DBACCESS_DENIED_ERROR | OperationalError | 8.4: 42000, 9.7: 42000 | dbaccess denied error |
| 1045 | ER_ACCESS_DENIED_ERROR | OperationalError | 8.4: 28000, 9.7: 28000 | access denied error |
| 1046 | ER_NO_DB_ERROR | DatabaseError | 8.4: 3D000, 9.7: 3D000 | no database error |
| 1047 | ER_UNKNOWN_COM_ERROR | OperationalError | 8.4: 08S01, 9.7: 08S01 | unknown com error |
| 1048 | ER_BAD_NULL_ERROR | IntegrityError | 8.4: 23000, 9.7: 23000 | bad null error |
| 1049 | ER_BAD_DB_ERROR | ProgrammingError | 8.4: 42000, 9.7: 42000 | bad database error |
| 1050 | ER_TABLE_EXISTS_ERROR | ProgrammingError | 8.4: 42S01, 9.7: 42S01 | table exists error |
| 1051 | ER_BAD_TABLE_ERROR | ProgrammingError | 8.4: 42S02, 9.7: 42S02 | bad table error |
| 1052 | ER_NON_UNIQ_ERROR | IntegrityError | 8.4: 23000, 9.7: 23000 | non uniq error |
| 1053 | ER_SERVER_SHUTDOWN | OperationalError | 8.4: 08S01, 9.7: 08S01 | server shutdown |
| 1054 | ER_BAD_FIELD_ERROR | ProgrammingError | 8.4: 42S22, 9.7: 42S22 | bad field error |
| 1055 | ER_WRONG_FIELD_WITH_GROUP | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid field with group |
| 1056 | ER_WRONG_GROUP_FIELD | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid group field |
| 1057 | ER_WRONG_SUM_SELECT | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid sum select |
| 1058 | ER_WRONG_VALUE_COUNT | ProgrammingError | 8.4: 21S01, 9.7: 21S01 | invalid value count |
| 1059 | ER_TOO_LONG_IDENT | ProgrammingError | 8.4: 42000, 9.7: 42000 | too long ident |
| 1060 | ER_DUP_FIELDNAME | ProgrammingError | 8.4: 42S21, 9.7: 42S21 | dup fieldname |
| 1061 | ER_DUP_KEYNAME | ProgrammingError | 8.4: 42000, 9.7: 42000 | dup keyname |
| 1062 | ER_DUP_ENTRY | IntegrityError | 8.4: 23000, 9.7: 23000 | dup entry |
| 1063 | ER_WRONG_FIELD_SPEC | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid field spec |
| 1064 | ER_PARSE_ERROR | ProgrammingError | 8.4: 42000, 9.7: 42000 | parse error |
| 1065 | ER_EMPTY_QUERY | ProgrammingError | 8.4: 42000, 9.7: 42000 | empty query |
| 1066 | ER_NONUNIQ_TABLE | ProgrammingError | 8.4: 42000, 9.7: 42000 | nonuniq table |
| 1067 | ER_INVALID_DEFAULT | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid default |
| 1068 | ER_MULTIPLE_PRI_KEY | ProgrammingError | 8.4: 42000, 9.7: 42000 | multiple pri key |
| 1069 | ER_TOO_MANY_KEYS | ProgrammingError | 8.4: 42000, 9.7: 42000 | too many keys |
| 1070 | ER_TOO_MANY_KEY_PARTS | ProgrammingError | 8.4: 42000, 9.7: 42000 | too many key parts |
| 1071 | ER_TOO_LONG_KEY | ProgrammingError | 8.4: 42000, 9.7: 42000 | too long key |
| 1072 | ER_KEY_COLUMN_DOES_NOT_EXITS | ProgrammingError | 8.4: 42000, 9.7: 42000 | key column does not exits |
| 1073 | ER_BLOB_USED_AS_KEY | ProgrammingError | 8.4: 42000, 9.7: 42000 | blob used as key |
| 1074 | ER_TOO_BIG_FIELDLENGTH | ProgrammingError | 8.4: 42000, 9.7: 42000 | too big fieldlength |
| 1075 | ER_WRONG_AUTO_KEY | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid auto key |
| 1076 | ER_READY | DatabaseError | 8.4: HY000, 9.7: HY000 | ready |
| 1079 | ER_SHUTDOWN_COMPLETE | DatabaseError | 8.4: HY000, 9.7: HY000 | shutdown complete |
| 1080 | ER_FORCING_CLOSE | OperationalError | 8.4: 08S01, 9.7: 08S01 | forcing close |
| 1081 | ER_IPSOCK_ERROR | OperationalError | 8.4: 08S01, 9.7: 08S01 | ipsock error |
| 1082 | ER_NO_SUCH_INDEX | ProgrammingError | 8.4: 42S12, 9.7: 42S12 | no such index |
| 1083 | ER_WRONG_FIELD_TERMINATORS | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid field terminators |
| 1084 | ER_BLOBS_AND_NO_TERMINATED | ProgrammingError | 8.4: 42000, 9.7: 42000 | blobs and no terminated |
| 1085 | ER_TEXTFILE_NOT_READABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | textfile not readable |
| 1086 | ER_FILE_EXISTS_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | file exists error |
| 1087 | ER_LOAD_INFO | DatabaseError | 8.4: HY000, 9.7: HY000 | load info |
| 1088 | ER_ALTER_INFO | DatabaseError | 8.4: HY000, 9.7: HY000 | alter info |
| 1089 | ER_WRONG_SUB_KEY | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid sub key |
| 1090 | ER_CANT_REMOVE_ALL_FIELDS | ProgrammingError | 8.4: 42000, 9.7: 42000 | cannot remove all fields |
| 1091 | ER_CANT_DROP_FIELD_OR_KEY | ProgrammingError | 8.4: 42000, 9.7: 42000 | cannot drop field or key |
| 1092 | ER_INSERT_INFO | DatabaseError | 8.4: HY000, 9.7: HY000 | insert info |
| 1093 | ER_UPDATE_TABLE_USED | DatabaseError | 8.4: HY000, 9.7: HY000 | update table used |
| 1094 | ER_NO_SUCH_THREAD | DatabaseError | 8.4: HY000, 9.7: HY000 | no such thread |
| 1095 | ER_KILL_DENIED_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | kill denied error |
| 1096 | ER_NO_TABLES_USED | DatabaseError | 8.4: HY000, 9.7: HY000 | no tables used |
| 1097 | ER_TOO_BIG_SET | DatabaseError | 8.4: HY000, 9.7: HY000 | too big set |
| 1098 | ER_NO_UNIQUE_LOGFILE | DatabaseError | 8.4: HY000, 9.7: HY000 | no unique logfile |
| 1099 | ER_TABLE_NOT_LOCKED_FOR_WRITE | DatabaseError | 8.4: HY000, 9.7: HY000 | table not locked for write |
| 1100 | ER_TABLE_NOT_LOCKED | DatabaseError | 8.4: HY000, 9.7: HY000 | table not locked |
| 1101 | ER_BLOB_CANT_HAVE_DEFAULT | ProgrammingError | 8.4: 42000, 9.7: 42000 | blob cannot have default |
| 1102 | ER_WRONG_DB_NAME | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid database name |
| 1103 | ER_WRONG_TABLE_NAME | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid table name |
| 1104 | ER_TOO_BIG_SELECT | ProgrammingError | 8.4: 42000, 9.7: 42000 | too big select |
| 1105 | ER_UNKNOWN_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | unknown error |
| 1106 | ER_UNKNOWN_PROCEDURE | ProgrammingError | 8.4: 42000, 9.7: 42000 | unknown procedure |
| 1107 | ER_WRONG_PARAMCOUNT_TO_PROCEDURE | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid paramcount to procedure |
| 1108 | ER_WRONG_PARAMETERS_TO_PROCEDURE | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid parameters to procedure |
| 1109 | ER_UNKNOWN_TABLE | ProgrammingError | 8.4: 42S02, 9.7: 42S02 | unknown table |
| 1110 | ER_FIELD_SPECIFIED_TWICE | ProgrammingError | 8.4: 42000, 9.7: 42000 | field specified twice |
| 1111 | ER_INVALID_GROUP_FUNC_USE | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid group function use |
| 1112 | ER_UNSUPPORTED_EXTENSION | NotSupportedError | 8.4: 42000, 9.7: 42000 | unsupported extension |
| 1113 | ER_TABLE_MUST_HAVE_COLUMNS | ProgrammingError | 8.4: 42000, 9.7: 42000 | table must have columns |
| 1114 | ER_RECORD_FILE_FULL | OperationalError | 8.4: HY000, 9.7: HY000 | record file full |
| 1115 | ER_UNKNOWN_CHARACTER_SET | ProgrammingError | 8.4: 42000, 9.7: 42000 | unknown character set |
| 1116 | ER_TOO_MANY_TABLES | DatabaseError | 8.4: HY000, 9.7: HY000 | too many tables |
| 1117 | ER_TOO_MANY_FIELDS | DatabaseError | 8.4: HY000, 9.7: HY000 | too many fields |
| 1118 | ER_TOO_BIG_ROWSIZE | ProgrammingError | 8.4: 42000, 9.7: 42000 | too big rowsize |
| 1119 | ER_STACK_OVERRUN | OperationalError | 8.4: HY000, 9.7: HY000 | stack overrun |
| 1120 | ER_WRONG_OUTER_JOIN_UNUSED | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid outer join unused |
| 1121 | ER_NULL_COLUMN_IN_INDEX | ProgrammingError | 8.4: 42000, 9.7: 42000 | null column in index |
| 1122 | ER_CANT_FIND_UDF | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot find UDF |
| 1123 | ER_CANT_INITIALIZE_UDF | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot initialize UDF |
| 1124 | ER_UDF_NO_PATHS | DatabaseError | 8.4: HY000, 9.7: HY000 | UDF no paths |
| 1125 | ER_UDF_EXISTS | DatabaseError | 8.4: HY000, 9.7: HY000 | UDF exists |
| 1126 | ER_CANT_OPEN_LIBRARY | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot open library |
| 1127 | ER_CANT_FIND_DL_ENTRY | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot find dl entry |
| 1128 | ER_FUNCTION_NOT_DEFINED | DatabaseError | 8.4: HY000, 9.7: HY000 | function not defined |
| 1129 | ER_HOST_IS_BLOCKED | DatabaseError | 8.4: HY000, 9.7: HY000 | host is blocked |
| 1130 | ER_HOST_NOT_PRIVILEGED | DatabaseError | 8.4: HY000, 9.7: HY000 | host not privileged |
| 1131 | ER_PASSWORD_ANONYMOUS_USER | ProgrammingError | 8.4: 42000, 9.7: 42000 | password anonymous user |
| 1132 | ER_PASSWORD_NOT_ALLOWED | ProgrammingError | 8.4: 42000, 9.7: 42000 | password not allowed |
| 1133 | ER_PASSWORD_NO_MATCH | ProgrammingError | 8.4: 42000, 9.7: 42000 | password no match |
| 1134 | ER_UPDATE_INFO | DatabaseError | 8.4: HY000, 9.7: HY000 | update info |
| 1135 | ER_CANT_CREATE_THREAD | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot create thread |
| 1136 | ER_WRONG_VALUE_COUNT_ON_ROW | ProgrammingError | 8.4: 21S01, 9.7: 21S01 | invalid value count on row |
| 1137 | ER_CANT_REOPEN_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot reopen table |
| 1138 | ER_INVALID_USE_OF_NULL | DataError | 8.4: 22004, 9.7: 22004 | invalid use of null |
| 1139 | ER_REGEXP_ERROR | ProgrammingError | 8.4: 42000, 9.7: 42000 | regexp error |
| 1140 | ER_MIX_OF_GROUP_FUNC_AND_FIELDS | ProgrammingError | 8.4: 42000, 9.7: 42000 | mix of group function and fields |
| 1141 | ER_NONEXISTING_GRANT | ProgrammingError | 8.4: 42000, 9.7: 42000 | nonexisting grant |
| 1142 | ER_TABLEACCESS_DENIED_ERROR | OperationalError | 8.4: 42000, 9.7: 42000 | tableaccess denied error |
| 1143 | ER_COLUMNACCESS_DENIED_ERROR | OperationalError | 8.4: 42000, 9.7: 42000 | columnaccess denied error |
| 1144 | ER_ILLEGAL_GRANT_FOR_TABLE | ProgrammingError | 8.4: 42000, 9.7: 42000 | illegal grant for table |
| 1145 | ER_GRANT_WRONG_HOST_OR_USER | ProgrammingError | 8.4: 42000, 9.7: 42000 | grant invalid host or user |
| 1146 | ER_NO_SUCH_TABLE | ProgrammingError | 8.4: 42S02, 9.7: 42S02 | no such table |
| 1147 | ER_NONEXISTING_TABLE_GRANT | ProgrammingError | 8.4: 42000, 9.7: 42000 | nonexisting table grant |
| 1148 | ER_NOT_ALLOWED_COMMAND | ProgrammingError | 8.4: 42000, 9.7: 42000 | not allowed command |
| 1149 | ER_SYNTAX_ERROR | ProgrammingError | 8.4: 42000, 9.7: 42000 | syntax error |
| 1152 | ER_ABORTING_CONNECTION | OperationalError | 8.4: 08S01, 9.7: 08S01 | aborting connection |
| 1153 | ER_NET_PACKET_TOO_LARGE | OperationalError | 8.4: 08S01, 9.7: 08S01 | net packet too large |
| 1154 | ER_NET_READ_ERROR_FROM_PIPE | OperationalError | 8.4: 08S01, 9.7: 08S01 | net read error from pipe |
| 1155 | ER_NET_FCNTL_ERROR | OperationalError | 8.4: 08S01, 9.7: 08S01 | net fcntl error |
| 1156 | ER_NET_PACKETS_OUT_OF_ORDER | OperationalError | 8.4: 08S01, 9.7: 08S01 | net packets out of order |
| 1157 | ER_NET_UNCOMPRESS_ERROR | OperationalError | 8.4: 08S01, 9.7: 08S01 | net uncompress error |
| 1158 | ER_NET_READ_ERROR | OperationalError | 8.4: 08S01, 9.7: 08S01 | net read error |
| 1159 | ER_NET_READ_INTERRUPTED | OperationalError | 8.4: 08S01, 9.7: 08S01 | net read interrupted |
| 1160 | ER_NET_ERROR_ON_WRITE | OperationalError | 8.4: 08S01, 9.7: 08S01 | net error on write |
| 1161 | ER_NET_WRITE_INTERRUPTED | OperationalError | 8.4: 08S01, 9.7: 08S01 | net write interrupted |
| 1162 | ER_TOO_LONG_STRING | ProgrammingError | 8.4: 42000, 9.7: 42000 | too long string |
| 1163 | ER_TABLE_CANT_HANDLE_BLOB | ProgrammingError | 8.4: 42000, 9.7: 42000 | table cannot handle blob |
| 1164 | ER_TABLE_CANT_HANDLE_AUTO_INCREMENT | ProgrammingError | 8.4: 42000, 9.7: 42000 | table cannot handle auto increment |
| 1166 | ER_WRONG_COLUMN_NAME | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid column name |
| 1167 | ER_WRONG_KEY_COLUMN | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid key column |
| 1168 | ER_WRONG_MRG_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid mrg table |
| 1169 | ER_DUP_UNIQUE | IntegrityError | 8.4: 23000, 9.7: 23000 | dup unique |
| 1170 | ER_BLOB_KEY_WITHOUT_LENGTH | ProgrammingError | 8.4: 42000, 9.7: 42000 | blob key without length |
| 1171 | ER_PRIMARY_CANT_HAVE_NULL | ProgrammingError | 8.4: 42000, 9.7: 42000 | primary cannot have null |
| 1172 | ER_TOO_MANY_ROWS | ProgrammingError | 8.4: 42000, 9.7: 42000 | too many rows |
| 1173 | ER_REQUIRES_PRIMARY_KEY | ProgrammingError | 8.4: 42000, 9.7: 42000 | requires primary key |
| 1175 | ER_UPDATE_WITHOUT_KEY_IN_SAFE_MODE | DatabaseError | 8.4: HY000, 9.7: HY000 | update without key in safe mode |
| 1176 | ER_KEY_DOES_NOT_EXITS | ProgrammingError | 8.4: 42000, 9.7: 42000 | key does not exits |
| 1177 | ER_CHECK_NO_SUCH_TABLE | ProgrammingError | 8.4: 42000, 9.7: 42000 | check no such table |
| 1178 | ER_CHECK_NOT_IMPLEMENTED | ProgrammingError | 8.4: 42000, 9.7: 42000 | check not implemented |
| 1179 | ER_CANT_DO_THIS_DURING_AN_TRANSACTION | ProgrammingError | 8.4: 25000, 9.7: 25000 | cannot do this during an transaction |
| 1180 | ER_ERROR_DURING_COMMIT | DatabaseError | 8.4: HY000, 9.7: HY000 | error during commit |
| 1181 | ER_ERROR_DURING_ROLLBACK | DatabaseError | 8.4: HY000, 9.7: HY000 | error during rollback |
| 1182 | ER_ERROR_DURING_FLUSH_LOGS | DatabaseError | 8.4: HY000, 9.7: HY000 | error during flush logs |
| 1184 | ER_NEW_ABORTING_CONNECTION | OperationalError | 8.4: 08S01, 9.7: 08S01 | new aborting connection |
| 1188 | ER_SOURCE | DatabaseError | 8.4: HY000, 9.7: HY000 | source |
| 1189 | ER_SOURCE_NET_READ | OperationalError | 8.4: 08S01, 9.7: 08S01 | source net read |
| 1190 | ER_SOURCE_NET_WRITE | OperationalError | 8.4: 08S01, 9.7: 08S01 | source net write |
| 1191 | ER_FT_MATCHING_KEY_NOT_FOUND | DatabaseError | 8.4: HY000, 9.7: HY000 | ft matching key not found |
| 1192 | ER_LOCK_OR_ACTIVE_TRANSACTION | DatabaseError | 8.4: HY000, 9.7: HY000 | lock or active transaction |
| 1193 | ER_UNKNOWN_SYSTEM_VARIABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | unknown system variable |
| 1194 | ER_CRASHED_ON_USAGE | InternalError | 8.4: HY000, 9.7: HY000 | crashed on usage |
| 1195 | ER_CRASHED_ON_REPAIR | InternalError | 8.4: HY000, 9.7: HY000 | crashed on repair |
| 1196 | ER_WARNING_NOT_COMPLETE_ROLLBACK | DatabaseError | 8.4: HY000, 9.7: HY000 | warning not complete rollback |
| 1197 | ER_TRANS_CACHE_FULL | DatabaseError | 8.4: HY000, 9.7: HY000 | trans cache full |
| 1199 | ER_REPLICA_NOT_RUNNING | DatabaseError | 8.4: HY000, 9.7: HY000 | replica not running |
| 1200 | ER_BAD_REPLICA | DatabaseError | 8.4: HY000, 9.7: HY000 | bad replica |
| 1201 | ER_CONNECTION_METADATA | DatabaseError | 8.4: HY000, 9.7: HY000 | connection metadata |
| 1202 | ER_REPLICA_THREAD | DatabaseError | 8.4: HY000, 9.7: HY000 | replica thread |
| 1203 | ER_TOO_MANY_USER_CONNECTIONS | OperationalError | 8.4: 42000, 9.7: 42000 | too many user connections |
| 1204 | ER_SET_CONSTANTS_ONLY | DatabaseError | 8.4: HY000, 9.7: HY000 | set constants only |
| 1205 | ER_LOCK_WAIT_TIMEOUT | OperationalError | 8.4: HY000, 9.7: HY000 | lock wait timeout |
| 1206 | ER_LOCK_TABLE_FULL | OperationalError | 8.4: HY000, 9.7: HY000 | lock table full |
| 1207 | ER_READ_ONLY_TRANSACTION | ProgrammingError | 8.4: 25000, 9.7: 25000 | read only transaction |
| 1210 | ER_WRONG_ARGUMENTS | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid arguments |
| 1211 | ER_NO_PERMISSION_TO_CREATE_USER | ProgrammingError | 8.4: 42000, 9.7: 42000 | no permission to create user |
| 1213 | ER_LOCK_DEADLOCK | OperationalError | 8.4: 40001, 9.7: 40001 | lock deadlock |
| 1214 | ER_TABLE_CANT_HANDLE_FT | DatabaseError | 8.4: HY000, 9.7: HY000 | table cannot handle ft |
| 1215 | ER_CANNOT_ADD_FOREIGN | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot add foreign |
| 1216 | ER_NO_REFERENCED_ROW | IntegrityError | 8.4: 23000, 9.7: 23000 | no referenced row |
| 1217 | ER_ROW_IS_REFERENCED | IntegrityError | 8.4: 23000, 9.7: 23000 | row is referenced |
| 1218 | ER_CONNECT_TO_SOURCE | OperationalError | 8.4: 08S01, 9.7: 08S01 | connect to source |
| 1220 | ER_ERROR_WHEN_EXECUTING_COMMAND | DatabaseError | 8.4: HY000, 9.7: HY000 | error when executing command |
| 1221 | ER_WRONG_USAGE | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid usage |
| 1222 | ER_WRONG_NUMBER_OF_COLUMNS_IN_SELECT | ProgrammingError | 8.4: 21000, 9.7: 21000 | invalid number of columns in select |
| 1223 | ER_CANT_UPDATE_WITH_READLOCK | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot update with readlock |
| 1224 | ER_MIXING_NOT_ALLOWED | DatabaseError | 8.4: HY000, 9.7: HY000 | mixing not allowed |
| 1225 | ER_DUP_ARGUMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | dup argument |
| 1226 | ER_USER_LIMIT_REACHED | OperationalError | 8.4: 42000, 9.7: 42000 | user limit reached |
| 1227 | ER_SPECIFIC_ACCESS_DENIED_ERROR | OperationalError | 8.4: 42000, 9.7: 42000 | specific access denied error |
| 1228 | ER_LOCAL_VARIABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | local variable |
| 1229 | ER_GLOBAL_VARIABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | global variable |
| 1230 | ER_NO_DEFAULT | ProgrammingError | 8.4: 42000, 9.7: 42000 | no default |
| 1231 | ER_WRONG_VALUE_FOR_VAR | DataError | 8.4: 42000, 9.7: 42000 | invalid value for variable |
| 1232 | ER_WRONG_TYPE_FOR_VAR | DataError | 8.4: 42000, 9.7: 42000 | invalid type for variable |
| 1233 | ER_VAR_CANT_BE_READ | DatabaseError | 8.4: HY000, 9.7: HY000 | variable cannot be read |
| 1234 | ER_CANT_USE_OPTION_HERE | ProgrammingError | 8.4: 42000, 9.7: 42000 | cannot use option here |
| 1235 | ER_NOT_SUPPORTED_YET | NotSupportedError | 8.4: 42000, 9.7: 42000 | not supported yet |
| 1236 | ER_SOURCE_FATAL_ERROR_READING_BINLOG | DatabaseError | 8.4: HY000, 9.7: HY000 | source fatal error reading binlog |
| 1237 | ER_REPLICA_IGNORED_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | replica ignored table |
| 1238 | ER_INCORRECT_GLOBAL_LOCAL_VAR | DatabaseError | 8.4: HY000, 9.7: HY000 | incorrect global local variable |
| 1239 | ER_WRONG_FK_DEF | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid foreign key def |
| 1240 | ER_KEY_REF_DO_NOT_MATCH_TABLE_REF | DatabaseError | 8.4: HY000, 9.7: HY000 | key reference do not match table reference |
| 1241 | ER_OPERAND_COLUMNS | ProgrammingError | 8.4: 21000, 9.7: 21000 | operand columns |
| 1242 | ER_SUBQUERY_NO_1_ROW | ProgrammingError | 8.4: 21000, 9.7: 21000 | subquery no 1 row |
| 1243 | ER_UNKNOWN_STMT_HANDLER | DatabaseError | 8.4: HY000, 9.7: HY000 | unknown statement handler |
| 1244 | ER_CORRUPT_HELP_DB | DatabaseError | 8.4: HY000, 9.7: HY000 | corrupt help database |
| 1246 | ER_AUTO_CONVERT | DatabaseError | 8.4: HY000, 9.7: HY000 | auto convert |
| 1247 | ER_ILLEGAL_REFERENCE | ProgrammingError | 8.4: 42S22, 9.7: 42S22 | illegal reference |
| 1248 | ER_DERIVED_MUST_HAVE_ALIAS | ProgrammingError | 8.4: 42000, 9.7: 42000 | derived must have alias |
| 1249 | ER_SELECT_REDUCED | DatabaseError | 8.4: 01000, 9.7: 01000 | select reduced |
| 1250 | ER_TABLENAME_NOT_ALLOWED_HERE | ProgrammingError | 8.4: 42000, 9.7: 42000 | tablename not allowed here |
| 1251 | ER_NOT_SUPPORTED_AUTH_MODE | OperationalError | 8.4: 08004, 9.7: 08004 | not supported auth mode |
| 1252 | ER_SPATIAL_CANT_HAVE_NULL | ProgrammingError | 8.4: 42000, 9.7: 42000 | spatial cannot have null |
| 1253 | ER_COLLATION_CHARSET_MISMATCH | ProgrammingError | 8.4: 42000, 9.7: 42000 | collation character set mismatch |
| 1256 | ER_TOO_BIG_FOR_UNCOMPRESS | DatabaseError | 8.4: HY000, 9.7: HY000 | too big for uncompress |
| 1257 | ER_ZLIB_Z_MEM_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | zlib z mem error |
| 1258 | ER_ZLIB_Z_BUF_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | zlib z buf error |
| 1259 | ER_ZLIB_Z_DATA_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | zlib z data error |
| 1260 | ER_CUT_VALUE_GROUP_CONCAT | DatabaseError | 8.4: HY000, 9.7: HY000 | cut value group concat |
| 1261 | ER_WARN_TOO_FEW_RECORDS | DatabaseError | 8.4: 01000, 9.7: 01000 | warn too few records |
| 1262 | ER_WARN_TOO_MANY_RECORDS | DatabaseError | 8.4: 01000, 9.7: 01000 | warn too many records |
| 1263 | ER_WARN_NULL_TO_NOTNULL | IntegrityError | 8.4: 22004, 9.7: 22004 | warn null to notnull |
| 1264 | ER_WARN_DATA_OUT_OF_RANGE | DataError | 8.4: 22003, 9.7: 22003 | warn data out of range |
| 1265 | WARN_DATA_TRUNCATED | DataError | 8.4: 01000, 9.7: 01000 | data truncated |
| 1266 | ER_WARN_USING_OTHER_HANDLER | DatabaseError | 8.4: HY000, 9.7: HY000 | warn using other handler |
| 1267 | ER_CANT_AGGREGATE_2COLLATIONS | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot aggregate 2collations |
| 1269 | ER_REVOKE_GRANTS | DatabaseError | 8.4: HY000, 9.7: HY000 | revoke grants |
| 1270 | ER_CANT_AGGREGATE_3COLLATIONS | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot aggregate 3collations |
| 1271 | ER_CANT_AGGREGATE_NCOLLATIONS | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot aggregate ncollations |
| 1272 | ER_VARIABLE_IS_NOT_STRUCT | DatabaseError | 8.4: HY000, 9.7: HY000 | variable is not struct |
| 1273 | ER_UNKNOWN_COLLATION | DatabaseError | 8.4: HY000, 9.7: HY000 | unknown collation |
| 1274 | ER_REPLICA_IGNORED_SSL_PARAMS | DatabaseError | 8.4: HY000, 9.7: HY000 | replica ignored SSL params |
| 1276 | ER_WARN_FIELD_RESOLVED | DatabaseError | 8.4: HY000, 9.7: HY000 | warn field resolved |
| 1277 | ER_BAD_REPLICA_UNTIL_COND | DatabaseError | 8.4: HY000, 9.7: HY000 | bad replica until cond |
| 1278 | ER_MISSING_SKIP_REPLICA | DatabaseError | 8.4: HY000, 9.7: HY000 | missing skip replica |
| 1279 | ER_UNTIL_COND_IGNORED | DatabaseError | 8.4: HY000, 9.7: HY000 | until cond ignored |
| 1280 | ER_WRONG_NAME_FOR_INDEX | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid name for index |
| 1281 | ER_WRONG_NAME_FOR_CATALOG | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid name for catalog |
| 1283 | ER_BAD_FT_COLUMN | DatabaseError | 8.4: HY000, 9.7: HY000 | bad ft column |
| 1284 | ER_UNKNOWN_KEY_CACHE | DatabaseError | 8.4: HY000, 9.7: HY000 | unknown key cache |
| 1285 | ER_WARN_HOSTNAME_WONT_WORK | DatabaseError | 8.4: HY000, 9.7: HY000 | warn hostname wont work |
| 1286 | ER_UNKNOWN_STORAGE_ENGINE | NotSupportedError | 8.4: 42000, 9.7: 42000 | unknown storage engine |
| 1287 | ER_WARN_DEPRECATED_SYNTAX | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated syntax |
| 1288 | ER_NON_UPDATABLE_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | non updatable table |
| 1289 | ER_FEATURE_DISABLED | NotSupportedError | 8.4: HY000, 9.7: HY000 | feature disabled |
| 1290 | ER_OPTION_PREVENTS_STATEMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | option prevents statement |
| 1291 | ER_DUPLICATED_VALUE_IN_TYPE | DatabaseError | 8.4: HY000, 9.7: HY000 | duplicated value in type |
| 1292 | ER_TRUNCATED_WRONG_VALUE | DataError | 8.4: 22007, 9.7: 22007 | truncated invalid value |
| 1294 | ER_INVALID_ON_UPDATE | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid on update |
| 1295 | ER_UNSUPPORTED_PS | DatabaseError | 8.4: HY000, 9.7: HY000 | unsupported ps |
| 1296 | ER_GET_ERRMSG | DatabaseError | 8.4: HY000, 9.7: HY000 | get errmsg |
| 1297 | ER_GET_TEMPORARY_ERRMSG | DatabaseError | 8.4: HY000, 9.7: HY000 | get temporary errmsg |
| 1298 | ER_UNKNOWN_TIME_ZONE | DatabaseError | 8.4: HY000, 9.7: HY000 | unknown time zone |
| 1299 | ER_WARN_INVALID_TIMESTAMP | DataError | 8.4: HY000, 9.7: HY000 | warn invalid timestamp |
| 1300 | ER_INVALID_CHARACTER_STRING | DataError | 8.4: HY000, 9.7: HY000 | invalid character string |
| 1301 | ER_WARN_ALLOWED_PACKET_OVERFLOWED | DatabaseError | 8.4: HY000, 9.7: HY000 | warn allowed packet overflowed |
| 1302 | ER_CONFLICTING_DECLARATIONS | DatabaseError | 8.4: HY000, 9.7: HY000 | conflicting declarations |
| 1303 | ER_SP_NO_RECURSIVE_CREATE | DatabaseError | 8.4: 2F003, 9.7: 2F003 | sp no recursive create |
| 1304 | ER_SP_ALREADY_EXISTS | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp already exists |
| 1305 | ER_SP_DOES_NOT_EXIST | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp does not exist |
| 1306 | ER_SP_DROP_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | sp drop failed |
| 1307 | ER_SP_STORE_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | sp store failed |
| 1308 | ER_SP_LILABEL_MISMATCH | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp lilabel mismatch |
| 1309 | ER_SP_LABEL_REDEFINE | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp label redefine |
| 1310 | ER_SP_LABEL_MISMATCH | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp label mismatch |
| 1311 | ER_SP_UNINIT_VAR | DatabaseError | 8.4: 01000, 9.7: 01000 | sp uninit variable |
| 1312 | ER_SP_BADSELECT | NotSupportedError | 8.4: 0A000, 9.7: 0A000 | sp badselect |
| 1313 | ER_SP_BADRETURN | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp badreturn |
| 1314 | ER_SP_BADSTATEMENT | NotSupportedError | 8.4: 0A000, 9.7: 0A000 | sp badstatement |
| 1315 | ER_UPDATE_LOG_DEPRECATED_IGNORED | ProgrammingError | 8.4: 42000, 9.7: 42000 | update log deprecated ignored |
| 1316 | ER_UPDATE_LOG_DEPRECATED_TRANSLATED | ProgrammingError | 8.4: 42000, 9.7: 42000 | update log deprecated translated |
| 1317 | ER_QUERY_INTERRUPTED | OperationalError | 8.4: 70100, 9.7: 70100 | query interrupted |
| 1318 | ER_SP_WRONG_NO_OF_ARGS | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp invalid no of arguments |
| 1319 | ER_SP_COND_MISMATCH | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp cond mismatch |
| 1320 | ER_SP_NORETURN | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp noreturn |
| 1321 | ER_SP_NORETURNEND | DatabaseError | 8.4: 2F005, 9.7: 2F005 | sp noreturnend |
| 1322 | ER_SP_BAD_CURSOR_QUERY | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp bad cursor query |
| 1323 | ER_SP_BAD_CURSOR_SELECT | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp bad cursor select |
| 1324 | ER_SP_CURSOR_MISMATCH | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp cursor mismatch |
| 1325 | ER_SP_CURSOR_ALREADY_OPEN | ProgrammingError | 8.4: 24000, 9.7: 24000 | sp cursor already open |
| 1326 | ER_SP_CURSOR_NOT_OPEN | ProgrammingError | 8.4: 24000, 9.7: 24000 | sp cursor not open |
| 1327 | ER_SP_UNDECLARED_VAR | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp undeclared variable |
| 1328 | ER_SP_WRONG_NO_OF_FETCH_ARGS | DatabaseError | 8.4: HY000, 9.7: HY000 | sp invalid no of fetch arguments |
| 1329 | ER_SP_FETCH_NO_DATA | DatabaseError | 8.4: 02000, 9.7: 02000 | sp fetch no data |
| 1330 | ER_SP_DUP_PARAM | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp dup param |
| 1331 | ER_SP_DUP_VAR | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp dup variable |
| 1332 | ER_SP_DUP_COND | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp dup cond |
| 1333 | ER_SP_DUP_CURS | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp dup curs |
| 1334 | ER_SP_CANT_ALTER | DatabaseError | 8.4: HY000, 9.7: HY000 | sp cannot alter |
| 1335 | ER_SP_SUBSELECT_NYI | NotSupportedError | 8.4: 0A000, 9.7: 0A000 | sp subselect nyi |
| 1336 | ER_STMT_NOT_ALLOWED_IN_SF_OR_TRG | NotSupportedError | 8.4: 0A000, 9.7: 0A000 | statement not allowed in sf or trg |
| 1337 | ER_SP_VARCOND_AFTER_CURSHNDLR | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp varcond after curshndlr |
| 1338 | ER_SP_CURSOR_AFTER_HANDLER | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp cursor after handler |
| 1339 | ER_SP_CASE_NOT_FOUND | DatabaseError | 8.4: 20000, 9.7: 20000 | sp case not found |
| 1340 | ER_FPARSER_TOO_BIG_FILE | DatabaseError | 8.4: HY000, 9.7: HY000 | fparser too big file |
| 1341 | ER_FPARSER_BAD_HEADER | DatabaseError | 8.4: HY000, 9.7: HY000 | fparser bad header |
| 1342 | ER_FPARSER_EOF_IN_COMMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | fparser eof in comment |
| 1343 | ER_FPARSER_ERROR_IN_PARAMETER | DatabaseError | 8.4: HY000, 9.7: HY000 | fparser error in parameter |
| 1344 | ER_FPARSER_EOF_IN_UNKNOWN_PARAMETER | DatabaseError | 8.4: HY000, 9.7: HY000 | fparser eof in unknown parameter |
| 1345 | ER_VIEW_NO_EXPLAIN | DatabaseError | 8.4: HY000, 9.7: HY000 | view no explain |
| 1347 | ER_WRONG_OBJECT | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid object |
| 1348 | ER_NONUPDATEABLE_COLUMN | DatabaseError | 8.4: HY000, 9.7: HY000 | nonupdateable column |
| 1350 | ER_VIEW_SELECT_CLAUSE | DatabaseError | 8.4: HY000, 9.7: HY000 | view select clause |
| 1351 | ER_VIEW_SELECT_VARIABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | view select variable |
| 1352 | ER_VIEW_SELECT_TMPTABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | view select tmptable |
| 1353 | ER_VIEW_WRONG_LIST | DatabaseError | 8.4: HY000, 9.7: HY000 | view invalid list |
| 1354 | ER_WARN_VIEW_MERGE | DatabaseError | 8.4: HY000, 9.7: HY000 | warn view merge |
| 1355 | ER_WARN_VIEW_WITHOUT_KEY | DatabaseError | 8.4: HY000, 9.7: HY000 | warn view without key |
| 1356 | ER_VIEW_INVALID | DatabaseError | 8.4: HY000, 9.7: HY000 | view invalid |
| 1357 | ER_SP_NO_DROP_SP | DatabaseError | 8.4: HY000, 9.7: HY000 | sp no drop sp |
| 1359 | ER_TRG_ALREADY_EXISTS | DatabaseError | 8.4: HY000, 9.7: HY000 | trg already exists |
| 1360 | ER_TRG_DOES_NOT_EXIST | DatabaseError | 8.4: HY000, 9.7: HY000 | trg does not exist |
| 1361 | ER_TRG_ON_VIEW_OR_TEMP_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | trg on view or temp table |
| 1362 | ER_TRG_CANT_CHANGE_ROW | DatabaseError | 8.4: HY000, 9.7: HY000 | trg cannot change row |
| 1363 | ER_TRG_NO_SUCH_ROW_IN_TRG | DatabaseError | 8.4: HY000, 9.7: HY000 | trg no such row in trg |
| 1364 | ER_NO_DEFAULT_FOR_FIELD | IntegrityError | 8.4: HY000, 9.7: HY000 | no default for field |
| 1365 | ER_DIVISION_BY_ZERO | DataError | 8.4: 22012, 9.7: 22012 | division by zero |
| 1366 | ER_TRUNCATED_WRONG_VALUE_FOR_FIELD | DataError | 8.4: HY000, 9.7: HY000 | truncated invalid value for field |
| 1367 | ER_ILLEGAL_VALUE_FOR_TYPE | DataError | 8.4: 22007, 9.7: 22007 | illegal value for type |
| 1368 | ER_VIEW_NONUPD_CHECK | DatabaseError | 8.4: HY000, 9.7: HY000 | view nonupd check |
| 1369 | ER_VIEW_CHECK_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | view check failed |
| 1370 | ER_PROCACCESS_DENIED_ERROR | OperationalError | 8.4: 42000, 9.7: 42000 | procaccess denied error |
| 1371 | ER_RELAY_LOG_FAIL | DatabaseError | 8.4: HY000, 9.7: HY000 | relay log fail |
| 1373 | ER_UNKNOWN_TARGET_BINLOG | DatabaseError | 8.4: HY000, 9.7: HY000 | unknown target binlog |
| 1374 | ER_IO_ERR_LOG_INDEX_READ | DatabaseError | 8.4: HY000, 9.7: HY000 | I/O err log index read |
| 1375 | ER_BINLOG_PURGE_PROHIBITED | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog purge prohibited |
| 1376 | ER_FSEEK_FAIL | DatabaseError | 8.4: HY000, 9.7: HY000 | fseek fail |
| 1377 | ER_BINLOG_PURGE_FATAL_ERR | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog purge fatal err |
| 1378 | ER_LOG_IN_USE | DatabaseError | 8.4: HY000, 9.7: HY000 | log in use |
| 1379 | ER_LOG_PURGE_UNKNOWN_ERR | DatabaseError | 8.4: HY000, 9.7: HY000 | log purge unknown err |
| 1380 | ER_RELAY_LOG_INIT | DatabaseError | 8.4: HY000, 9.7: HY000 | relay log init |
| 1381 | ER_NO_BINARY_LOGGING | DatabaseError | 8.4: HY000, 9.7: HY000 | no binary logging |
| 1382 | ER_RESERVED_SYNTAX | DatabaseError | 8.4: HY000, 9.7: HY000 | reserved syntax |
| 1390 | ER_PS_MANY_PARAM | DatabaseError | 8.4: HY000, 9.7: HY000 | ps many param |
| 1391 | ER_KEY_PART_0 | DatabaseError | 8.4: HY000, 9.7: HY000 | key part 0 |
| 1392 | ER_VIEW_CHECKSUM | DatabaseError | 8.4: HY000, 9.7: HY000 | view checksum |
| 1393 | ER_VIEW_MULTIUPDATE | DatabaseError | 8.4: HY000, 9.7: HY000 | view multiupdate |
| 1394 | ER_VIEW_NO_INSERT_FIELD_LIST | DatabaseError | 8.4: HY000, 9.7: HY000 | view no insert field list |
| 1395 | ER_VIEW_DELETE_MERGE_VIEW | DatabaseError | 8.4: HY000, 9.7: HY000 | view delete merge view |
| 1396 | ER_CANNOT_USER | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot user |
| 1397 | ER_XAER_NOTA | DatabaseError | 8.4: XAE04, 9.7: XAE04 | xaer nota |
| 1398 | ER_XAER_INVAL | DatabaseError | 8.4: XAE05, 9.7: XAE05 | xaer inval |
| 1399 | ER_XAER_RMFAIL | DatabaseError | 8.4: XAE07, 9.7: XAE07 | xaer rmfail |
| 1400 | ER_XAER_OUTSIDE | DatabaseError | 8.4: XAE09, 9.7: XAE09 | xaer outside |
| 1401 | ER_XAER_RMERR | DatabaseError | 8.4: XAE03, 9.7: XAE03 | xaer rmerr |
| 1402 | ER_XA_RBROLLBACK | DatabaseError | 8.4: XA100, 9.7: XA100 | XA rbrollback |
| 1403 | ER_NONEXISTING_PROC_GRANT | ProgrammingError | 8.4: 42000, 9.7: 42000 | nonexisting proc grant |
| 1404 | ER_PROC_AUTO_GRANT_FAIL | DatabaseError | 8.4: HY000, 9.7: HY000 | proc auto grant fail |
| 1405 | ER_PROC_AUTO_REVOKE_FAIL | DatabaseError | 8.4: HY000, 9.7: HY000 | proc auto revoke fail |
| 1406 | ER_DATA_TOO_LONG | DataError | 8.4: 22001, 9.7: 22001 | data too long |
| 1407 | ER_SP_BAD_SQLSTATE | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp bad sqlstate |
| 1408 | ER_STARTUP | DatabaseError | 8.4: HY000, 9.7: HY000 | startup |
| 1409 | ER_LOAD_FROM_FIXED_SIZE_ROWS_TO_VAR | DatabaseError | 8.4: HY000, 9.7: HY000 | load from fixed size rows to variable |
| 1410 | ER_CANT_CREATE_USER_WITH_GRANT | ProgrammingError | 8.4: 42000, 9.7: 42000 | cannot create user with grant |
| 1411 | ER_WRONG_VALUE_FOR_TYPE | DataError | 8.4: HY000, 9.7: HY000 | invalid value for type |
| 1412 | ER_TABLE_DEF_CHANGED | DatabaseError | 8.4: HY000, 9.7: HY000 | table def changed |
| 1413 | ER_SP_DUP_HANDLER | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp dup handler |
| 1414 | ER_SP_NOT_VAR_ARG | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp not variable argument |
| 1415 | ER_SP_NO_RETSET | NotSupportedError | 8.4: 0A000, 9.7: 0A000 | sp no retset |
| 1416 | ER_CANT_CREATE_GEOMETRY_OBJECT | DataError | 8.4: 22003, 9.7: 22003 | cannot create geometry object |
| 1418 | ER_BINLOG_UNSAFE_ROUTINE | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe routine |
| 1419 | ER_BINLOG_CREATE_ROUTINE_NEED_SUPER | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog create routine need super |
| 1421 | ER_STMT_HAS_NO_OPEN_CURSOR | DatabaseError | 8.4: HY000, 9.7: HY000 | statement has no open cursor |
| 1422 | ER_COMMIT_NOT_ALLOWED_IN_SF_OR_TRG | DatabaseError | 8.4: HY000, 9.7: HY000 | commit not allowed in sf or trg |
| 1423 | ER_NO_DEFAULT_FOR_VIEW_FIELD | DatabaseError | 8.4: HY000, 9.7: HY000 | no default for view field |
| 1424 | ER_SP_NO_RECURSION | DatabaseError | 8.4: HY000, 9.7: HY000 | sp no recursion |
| 1425 | ER_TOO_BIG_SCALE | ProgrammingError | 8.4: 42000, 9.7: 42000 | too big scale |
| 1426 | ER_TOO_BIG_PRECISION | ProgrammingError | 8.4: 42000, 9.7: 42000 | too big precision |
| 1427 | ER_M_BIGGER_THAN_D | ProgrammingError | 8.4: 42000, 9.7: 42000 | m bigger than d |
| 1428 | ER_WRONG_LOCK_OF_SYSTEM_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid lock of system table |
| 1429 | ER_CONNECT_TO_FOREIGN_DATA_SOURCE | DatabaseError | 8.4: HY000, 9.7: HY000 | connect to foreign data source |
| 1430 | ER_QUERY_ON_FOREIGN_DATA_SOURCE | DatabaseError | 8.4: HY000, 9.7: HY000 | query on foreign data source |
| 1431 | ER_FOREIGN_DATA_SOURCE_DOESNT_EXIST | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign data source doesnt exist |
| 1432 | ER_FOREIGN_DATA_STRING_INVALID_CANT_CREATE | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign data string invalid cannot create |
| 1433 | ER_FOREIGN_DATA_STRING_INVALID | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign data string invalid |
| 1435 | ER_TRG_IN_WRONG_SCHEMA | DatabaseError | 8.4: HY000, 9.7: HY000 | trg in invalid schema |
| 1436 | ER_STACK_OVERRUN_NEED_MORE | OperationalError | 8.4: HY000, 9.7: HY000 | stack overrun need more |
| 1437 | ER_TOO_LONG_BODY | ProgrammingError | 8.4: 42000, 9.7: 42000 | too long body |
| 1438 | ER_WARN_CANT_DROP_DEFAULT_KEYCACHE | DatabaseError | 8.4: HY000, 9.7: HY000 | warn cannot drop default keycache |
| 1439 | ER_TOO_BIG_DISPLAYWIDTH | ProgrammingError | 8.4: 42000, 9.7: 42000 | too big displaywidth |
| 1440 | ER_XAER_DUPID | DatabaseError | 8.4: XAE08, 9.7: XAE08 | xaer dupid |
| 1441 | ER_DATETIME_FUNCTION_OVERFLOW | DataError | 8.4: 22008, 9.7: 22008 | datetime function overflow |
| 1442 | ER_CANT_UPDATE_USED_TABLE_IN_SF_OR_TRG | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot update used table in sf or trg |
| 1443 | ER_VIEW_PREVENT_UPDATE | DatabaseError | 8.4: HY000, 9.7: HY000 | view prevent update |
| 1444 | ER_PS_NO_RECURSION | DatabaseError | 8.4: HY000, 9.7: HY000 | ps no recursion |
| 1445 | ER_SP_CANT_SET_AUTOCOMMIT | DatabaseError | 8.4: HY000, 9.7: HY000 | sp cannot set autocommit |
| 1447 | ER_VIEW_FRM_NO_USER | DatabaseError | 8.4: HY000, 9.7: HY000 | view frm no user |
| 1448 | ER_VIEW_OTHER_USER | DatabaseError | 8.4: HY000, 9.7: HY000 | view other user |
| 1449 | ER_NO_SUCH_USER | DatabaseError | 8.4: HY000, 9.7: HY000 | no such user |
| 1450 | ER_FORBID_SCHEMA_CHANGE | DatabaseError | 8.4: HY000, 9.7: HY000 | forbid schema change |
| 1451 | ER_ROW_IS_REFERENCED_2 | IntegrityError | 8.4: 23000, 9.7: 23000 | row is referenced 2 |
| 1452 | ER_NO_REFERENCED_ROW_2 | IntegrityError | 8.4: 23000, 9.7: 23000 | no referenced row 2 |
| 1453 | ER_SP_BAD_VAR_SHADOW | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp bad variable shadow |
| 1454 | ER_TRG_NO_DEFINER | DatabaseError | 8.4: HY000, 9.7: HY000 | trg no definer |
| 1455 | ER_OLD_FILE_FORMAT | DatabaseError | 8.4: HY000, 9.7: HY000 | old file format |
| 1456 | ER_SP_RECURSION_LIMIT | DatabaseError | 8.4: HY000, 9.7: HY000 | sp recursion limit |
| 1458 | ER_SP_WRONG_NAME | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp invalid name |
| 1459 | ER_TABLE_NEEDS_UPGRADE | DatabaseError | 8.4: HY000, 9.7: HY000 | table needs upgrade |
| 1460 | ER_SP_NO_AGGREGATE | ProgrammingError | 8.4: 42000, 9.7: 42000 | sp no aggregate |
| 1461 | ER_MAX_PREPARED_STMT_COUNT_REACHED | ProgrammingError | 8.4: 42000, 9.7: 42000 | max prepared statement count reached |
| 1462 | ER_VIEW_RECURSIVE | DatabaseError | 8.4: HY000, 9.7: HY000 | view recursive |
| 1463 | ER_NON_GROUPING_FIELD_USED | ProgrammingError | 8.4: 42000, 9.7: 42000 | non grouping field used |
| 1464 | ER_TABLE_CANT_HANDLE_SPKEYS | DatabaseError | 8.4: HY000, 9.7: HY000 | table cannot handle spkeys |
| 1465 | ER_NO_TRIGGERS_ON_SYSTEM_SCHEMA | DatabaseError | 8.4: HY000, 9.7: HY000 | no triggers on system schema |
| 1466 | ER_REMOVED_SPACES | DatabaseError | 8.4: HY000, 9.7: HY000 | removed spaces |
| 1467 | ER_AUTOINC_READ_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | autoinc read failed |
| 1468 | ER_USERNAME | DatabaseError | 8.4: HY000, 9.7: HY000 | username |
| 1469 | ER_HOSTNAME | DatabaseError | 8.4: HY000, 9.7: HY000 | hostname |
| 1470 | ER_WRONG_STRING_LENGTH | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid string length |
| 1471 | ER_NON_INSERTABLE_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | non insertable table |
| 1472 | ER_ADMIN_WRONG_MRG_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | admin invalid mrg table |
| 1473 | ER_TOO_HIGH_LEVEL_OF_NESTING_FOR_SELECT | DatabaseError | 8.4: HY000, 9.7: HY000 | too high level of nesting for select |
| 1474 | ER_NAME_BECOMES_EMPTY | DatabaseError | 8.4: HY000, 9.7: HY000 | name becomes empty |
| 1475 | ER_AMBIGUOUS_FIELD_TERM | DatabaseError | 8.4: HY000, 9.7: HY000 | ambiguous field term |
| 1476 | ER_FOREIGN_SERVER_EXISTS | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign server exists |
| 1477 | ER_FOREIGN_SERVER_DOESNT_EXIST | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign server doesnt exist |
| 1478 | ER_ILLEGAL_HA_CREATE_OPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | illegal ha create option |
| 1479 | ER_PARTITION_REQUIRES_VALUES_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | partition requires values error |
| 1480 | ER_PARTITION_WRONG_VALUES_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | partition invalid values error |
| 1481 | ER_PARTITION_MAXVALUE_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | partition maxvalue error |
| 1484 | ER_PARTITION_WRONG_NO_PART_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | partition invalid no part error |
| 1485 | ER_PARTITION_WRONG_NO_SUBPART_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | partition invalid no subpart error |
| 1486 | ER_WRONG_EXPR_IN_PARTITION_FUNC_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid expr in partition function error |
| 1488 | ER_FIELD_NOT_FOUND_PART_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | field not found part error |
| 1490 | ER_INCONSISTENT_PARTITION_INFO_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | inconsistent partition info error |
| 1491 | ER_PARTITION_FUNC_NOT_ALLOWED_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | partition function not allowed error |
| 1492 | ER_PARTITIONS_MUST_BE_DEFINED_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | partitions must be defined error |
| 1493 | ER_RANGE_NOT_INCREASING_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | range not increasing error |
| 1494 | ER_INCONSISTENT_TYPE_OF_FUNCTIONS_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | inconsistent type of functions error |
| 1495 | ER_MULTIPLE_DEF_CONST_IN_LIST_PART_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | multiple def const in list part error |
| 1496 | ER_PARTITION_ENTRY_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | partition entry error |
| 1497 | ER_MIX_HANDLER_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | mix handler error |
| 1498 | ER_PARTITION_NOT_DEFINED_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | partition not defined error |
| 1499 | ER_TOO_MANY_PARTITIONS_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | too many partitions error |
| 1500 | ER_SUBPARTITION_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | subpartition error |
| 1501 | ER_CANT_CREATE_HANDLER_FILE | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot create handler file |
| 1502 | ER_BLOB_FIELD_IN_PART_FUNC_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | blob field in part function error |
| 1503 | ER_UNIQUE_KEY_NEED_ALL_FIELDS_IN_PF | DatabaseError | 8.4: HY000, 9.7: HY000 | unique key need all fields in pf |
| 1504 | ER_NO_PARTS_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | no parts error |
| 1505 | ER_PARTITION_MGMT_ON_NONPARTITIONED | DatabaseError | 8.4: HY000, 9.7: HY000 | partition mgmt on nonpartitioned |
| 1506 | ER_FOREIGN_KEY_ON_PARTITIONED | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign key on partitioned |
| 1507 | ER_DROP_PARTITION_NON_EXISTENT | DatabaseError | 8.4: HY000, 9.7: HY000 | drop partition non existent |
| 1508 | ER_DROP_LAST_PARTITION | DatabaseError | 8.4: HY000, 9.7: HY000 | drop last partition |
| 1509 | ER_COALESCE_ONLY_ON_HASH_PARTITION | DatabaseError | 8.4: HY000, 9.7: HY000 | coalesce only on hash partition |
| 1510 | ER_REORG_HASH_ONLY_ON_SAME_NO | DatabaseError | 8.4: HY000, 9.7: HY000 | reorg hash only on same no |
| 1511 | ER_REORG_NO_PARAM_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | reorg no param error |
| 1512 | ER_ONLY_ON_RANGE_LIST_PARTITION | DatabaseError | 8.4: HY000, 9.7: HY000 | only on range list partition |
| 1513 | ER_ADD_PARTITION_SUBPART_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | add partition subpart error |
| 1514 | ER_ADD_PARTITION_NO_NEW_PARTITION | DatabaseError | 8.4: HY000, 9.7: HY000 | add partition no new partition |
| 1515 | ER_COALESCE_PARTITION_NO_PARTITION | DatabaseError | 8.4: HY000, 9.7: HY000 | coalesce partition no partition |
| 1516 | ER_REORG_PARTITION_NOT_EXIST | DatabaseError | 8.4: HY000, 9.7: HY000 | reorg partition not exist |
| 1517 | ER_SAME_NAME_PARTITION | DatabaseError | 8.4: HY000, 9.7: HY000 | same name partition |
| 1518 | ER_NO_BINLOG_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | no binlog error |
| 1519 | ER_CONSECUTIVE_REORG_PARTITIONS | DatabaseError | 8.4: HY000, 9.7: HY000 | consecutive reorg partitions |
| 1520 | ER_REORG_OUTSIDE_RANGE | DatabaseError | 8.4: HY000, 9.7: HY000 | reorg outside range |
| 1521 | ER_PARTITION_FUNCTION_FAILURE | DatabaseError | 8.4: HY000, 9.7: HY000 | partition function failure |
| 1523 | ER_LIMITED_PART_RANGE | DatabaseError | 8.4: HY000, 9.7: HY000 | limited part range |
| 1524 | ER_PLUGIN_IS_NOT_LOADED | NotSupportedError | 8.4: HY000, 9.7: HY000 | plugin is not loaded |
| 1525 | ER_WRONG_VALUE | DataError | 8.4: HY000, 9.7: HY000 | invalid value |
| 1526 | ER_NO_PARTITION_FOR_GIVEN_VALUE | DatabaseError | 8.4: HY000, 9.7: HY000 | no partition for given value |
| 1527 | ER_FILEGROUP_OPTION_ONLY_ONCE | DatabaseError | 8.4: HY000, 9.7: HY000 | filegroup option only once |
| 1528 | ER_CREATE_FILEGROUP_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | create filegroup failed |
| 1529 | ER_DROP_FILEGROUP_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | drop filegroup failed |
| 1530 | ER_TABLESPACE_AUTO_EXTEND_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | tablespace auto extend error |
| 1531 | ER_WRONG_SIZE_NUMBER | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid size number |
| 1532 | ER_SIZE_OVERFLOW_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | size overflow error |
| 1533 | ER_ALTER_FILEGROUP_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | alter filegroup failed |
| 1534 | ER_BINLOG_ROW_LOGGING_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog row logging failed |
| 1537 | ER_EVENT_ALREADY_EXISTS | DatabaseError | 8.4: HY000, 9.7: HY000 | event already exists |
| 1539 | ER_EVENT_DOES_NOT_EXIST | DatabaseError | 8.4: HY000, 9.7: HY000 | event does not exist |
| 1542 | ER_EVENT_INTERVAL_NOT_POSITIVE_OR_TOO_BIG | DatabaseError | 8.4: HY000, 9.7: HY000 | event interval not positive or too big |
| 1543 | ER_EVENT_ENDS_BEFORE_STARTS | DatabaseError | 8.4: HY000, 9.7: HY000 | event ends before starts |
| 1544 | ER_EVENT_EXEC_TIME_IN_THE_PAST | DatabaseError | 8.4: HY000, 9.7: HY000 | event exec time in the past |
| 1551 | ER_EVENT_SAME_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | event same name |
| 1553 | ER_DROP_INDEX_FK | DatabaseError | 8.4: HY000, 9.7: HY000 | drop index foreign key |
| 1554 | ER_WARN_DEPRECATED_SYNTAX_WITH_VER | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated syntax with ver |
| 1556 | ER_CANT_LOCK_LOG_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot lock log table |
| 1557 | ER_FOREIGN_DUPLICATE_KEY_OLD_UNUSED | IntegrityError | 8.4: 23000, 9.7: 23000 | foreign duplicate key old unused |
| 1558 | ER_COL_COUNT_DOESNT_MATCH_PLEASE_UPDATE | DatabaseError | 8.4: HY000, 9.7: HY000 | column count doesnt match please update |
| 1560 | ER_STORED_FUNCTION_PREVENTS_SWITCH_BINLOG_FORMAT | DatabaseError | 8.4: HY000, 9.7: HY000 | stored function prevents switch binlog format |
| 1562 | ER_PARTITION_NO_TEMPORARY | DatabaseError | 8.4: HY000, 9.7: HY000 | partition no temporary |
| 1563 | ER_PARTITION_CONST_DOMAIN_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | partition const domain error |
| 1564 | ER_PARTITION_FUNCTION_IS_NOT_ALLOWED | DatabaseError | 8.4: HY000, 9.7: HY000 | partition function is not allowed |
| 1566 | ER_NULL_IN_VALUES_LESS_THAN | DatabaseError | 8.4: HY000, 9.7: HY000 | null in values less than |
| 1567 | ER_WRONG_PARTITION_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid partition name |
| 1568 | ER_CANT_CHANGE_TX_CHARACTERISTICS | ProgrammingError | 8.4: 25001, 9.7: 25001 | cannot change transaction characteristics |
| 1569 | ER_DUP_ENTRY_AUTOINCREMENT_CASE | DatabaseError | 8.4: HY000, 9.7: HY000 | dup entry autoincrement case |
| 1571 | ER_EVENT_SET_VAR_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | event set variable error |
| 1572 | ER_PARTITION_MERGE_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | partition merge error |
| 1575 | ER_BASE64_DECODE_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | base64 decode error |
| 1576 | ER_EVENT_RECURSION_FORBIDDEN | DatabaseError | 8.4: HY000, 9.7: HY000 | event recursion forbidden |
| 1578 | ER_ONLY_INTEGERS_ALLOWED | DatabaseError | 8.4: HY000, 9.7: HY000 | only integers allowed |
| 1579 | ER_UNSUPORTED_LOG_ENGINE | DatabaseError | 8.4: HY000, 9.7: HY000 | unsuported log engine |
| 1580 | ER_BAD_LOG_STATEMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | bad log statement |
| 1581 | ER_CANT_RENAME_LOG_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot rename log table |
| 1582 | ER_WRONG_PARAMCOUNT_TO_NATIVE_FCT | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid paramcount to native fct |
| 1583 | ER_WRONG_PARAMETERS_TO_NATIVE_FCT | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid parameters to native fct |
| 1584 | ER_WRONG_PARAMETERS_TO_STORED_FCT | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid parameters to stored fct |
| 1585 | ER_NATIVE_FCT_NAME_COLLISION | DatabaseError | 8.4: HY000, 9.7: HY000 | native fct name collision |
| 1586 | ER_DUP_ENTRY_WITH_KEY_NAME | IntegrityError | 8.4: 23000, 9.7: 23000 | dup entry with key name |
| 1587 | ER_BINLOG_PURGE_EMFILE | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog purge emfile |
| 1588 | ER_EVENT_CANNOT_CREATE_IN_THE_PAST | DatabaseError | 8.4: HY000, 9.7: HY000 | event cannot create in the past |
| 1589 | ER_EVENT_CANNOT_ALTER_IN_THE_PAST | DatabaseError | 8.4: HY000, 9.7: HY000 | event cannot alter in the past |
| 1591 | ER_NO_PARTITION_FOR_GIVEN_VALUE_SILENT | DatabaseError | 8.4: HY000, 9.7: HY000 | no partition for given value silent |
| 1592 | ER_BINLOG_UNSAFE_STATEMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe statement |
| 1593 | ER_BINLOG_FATAL_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog fatal error |
| 1598 | ER_BINLOG_LOGGING_IMPOSSIBLE | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog logging impossible |
| 1599 | ER_VIEW_NO_CREATION_CTX | DatabaseError | 8.4: HY000, 9.7: HY000 | view no creation ctx |
| 1600 | ER_VIEW_INVALID_CREATION_CTX | DatabaseError | 8.4: HY000, 9.7: HY000 | view invalid creation ctx |
| 1602 | ER_TRG_CORRUPTED_FILE | DatabaseError | 8.4: HY000, 9.7: HY000 | trg corrupted file |
| 1603 | ER_TRG_NO_CREATION_CTX | DatabaseError | 8.4: HY000, 9.7: HY000 | trg no creation ctx |
| 1604 | ER_TRG_INVALID_CREATION_CTX | DatabaseError | 8.4: HY000, 9.7: HY000 | trg invalid creation ctx |
| 1605 | ER_EVENT_INVALID_CREATION_CTX | DatabaseError | 8.4: HY000, 9.7: HY000 | event invalid creation ctx |
| 1606 | ER_TRG_CANT_OPEN_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | trg cannot open table |
| 1609 | ER_NO_FORMAT_DESCRIPTION_EVENT_BEFORE_BINLOG_STATEMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | no format description event before binlog statement |
| 1610 | ER_REPLICA_CORRUPT_EVENT | DatabaseError | 8.4: HY000, 9.7: HY000 | replica corrupt event |
| 1612 | ER_LOG_PURGE_NO_FILE | DatabaseError | 8.4: HY000, 9.7: HY000 | log purge no file |
| 1613 | ER_XA_RBTIMEOUT | DatabaseError | 8.4: XA106, 9.7: XA106 | XA rbtimeout |
| 1614 | ER_XA_RBDEADLOCK | DatabaseError | 8.4: XA102, 9.7: XA102 | XA rbdeadlock |
| 1615 | ER_NEED_REPREPARE | DatabaseError | 8.4: HY000, 9.7: HY000 | need reprepare |
| 1617 | WARN_NO_CONNECTION_METADATA | DatabaseError | 8.4: HY000, 9.7: HY000 | no connection metadata |
| 1618 | WARN_OPTION_IGNORED | DatabaseError | 8.4: HY000, 9.7: HY000 | option ignored |
| 1619 | ER_PLUGIN_DELETE_BUILTIN | DatabaseError | 8.4: HY000, 9.7: HY000 | plugin delete builtin |
| 1620 | WARN_PLUGIN_BUSY | DatabaseError | 8.4: HY000, 9.7: HY000 | plugin busy |
| 1621 | ER_VARIABLE_IS_READONLY | DatabaseError | 8.4: HY000, 9.7: HY000 | variable is readonly |
| 1622 | ER_WARN_ENGINE_TRANSACTION_ROLLBACK | DatabaseError | 8.4: HY000, 9.7: HY000 | warn engine transaction rollback |
| 1624 | ER_REPLICA_HEARTBEAT_VALUE_OUT_OF_RANGE | DatabaseError | 8.4: HY000, 9.7: HY000 | replica heartbeat value out of range |
| 1625 | ER_NDB_REPLICATION_SCHEMA_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | NDB replication schema error |
| 1626 | ER_CONFLICT_FN_PARSE_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | conflict fn parse error |
| 1627 | ER_EXCEPTIONS_WRITE_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | exceptions write error |
| 1628 | ER_TOO_LONG_TABLE_COMMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | too long table comment |
| 1629 | ER_TOO_LONG_FIELD_COMMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | too long field comment |
| 1630 | ER_FUNC_INEXISTENT_NAME_COLLISION | ProgrammingError | 8.4: 42000, 9.7: 42000 | function inexistent name collision |
| 1631 | ER_DATABASE_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | database name |
| 1632 | ER_TABLE_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | table name |
| 1633 | ER_PARTITION_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | partition name |
| 1634 | ER_SUBPARTITION_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | subpartition name |
| 1635 | ER_TEMPORARY_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | temporary name |
| 1636 | ER_RENAMED_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | renamed name |
| 1637 | ER_TOO_MANY_CONCURRENT_TRXS | DatabaseError | 8.4: HY000, 9.7: HY000 | too many concurrent trxs |
| 1638 | WARN_NON_ASCII_SEPARATOR_NOT_IMPLEMENTED | DatabaseError | 8.4: HY000, 9.7: HY000 | non ascii separator not implemented |
| 1639 | ER_DEBUG_SYNC_TIMEOUT | DatabaseError | 8.4: HY000, 9.7: HY000 | debug sync timeout |
| 1640 | ER_DEBUG_SYNC_HIT_LIMIT | DatabaseError | 8.4: HY000, 9.7: HY000 | debug sync hit limit |
| 1641 | ER_DUP_SIGNAL_SET | ProgrammingError | 8.4: 42000, 9.7: 42000 | dup signal set |
| 1642 | ER_SIGNAL_WARN | DatabaseError | 8.4: 01000, 9.7: 01000 | signal warn |
| 1643 | ER_SIGNAL_NOT_FOUND | DatabaseError | 8.4: 02000, 9.7: 02000 | signal not found |
| 1644 | ER_SIGNAL_EXCEPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | signal exception |
| 1645 | ER_RESIGNAL_WITHOUT_ACTIVE_HANDLER | DatabaseError | 8.4: 0K000, 9.7: 0K000 | resignal without active handler |
| 1646 | ER_SIGNAL_BAD_CONDITION_TYPE | DatabaseError | 8.4: HY000, 9.7: HY000 | signal bad condition type |
| 1647 | WARN_COND_ITEM_TRUNCATED | DatabaseError | 8.4: HY000, 9.7: HY000 | cond item truncated |
| 1648 | ER_COND_ITEM_TOO_LONG | DatabaseError | 8.4: HY000, 9.7: HY000 | cond item too long |
| 1649 | ER_UNKNOWN_LOCALE | DatabaseError | 8.4: HY000, 9.7: HY000 | unknown locale |
| 1650 | ER_REPLICA_IGNORE_SERVER_IDS | DatabaseError | 8.4: HY000, 9.7: HY000 | replica ignore server ids |
| 1652 | ER_SAME_NAME_PARTITION_FIELD | DatabaseError | 8.4: HY000, 9.7: HY000 | same name partition field |
| 1653 | ER_PARTITION_COLUMN_LIST_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | partition column list error |
| 1654 | ER_WRONG_TYPE_COLUMN_VALUE_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid type column value error |
| 1655 | ER_TOO_MANY_PARTITION_FUNC_FIELDS_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | too many partition function fields error |
| 1656 | ER_MAXVALUE_IN_VALUES_IN | DatabaseError | 8.4: HY000, 9.7: HY000 | maxvalue in values in |
| 1657 | ER_TOO_MANY_VALUES_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | too many values error |
| 1658 | ER_ROW_SINGLE_PARTITION_FIELD_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | row single partition field error |
| 1659 | ER_FIELD_TYPE_NOT_ALLOWED_AS_PARTITION_FIELD | DatabaseError | 8.4: HY000, 9.7: HY000 | field type not allowed as partition field |
| 1660 | ER_PARTITION_FIELDS_TOO_LONG | DatabaseError | 8.4: HY000, 9.7: HY000 | partition fields too long |
| 1661 | ER_BINLOG_ROW_ENGINE_AND_STMT_ENGINE | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog row engine and statement engine |
| 1662 | ER_BINLOG_ROW_MODE_AND_STMT_ENGINE | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog row mode and statement engine |
| 1663 | ER_BINLOG_UNSAFE_AND_STMT_ENGINE | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe and statement engine |
| 1664 | ER_BINLOG_ROW_INJECTION_AND_STMT_ENGINE | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog row injection and statement engine |
| 1665 | ER_BINLOG_STMT_MODE_AND_ROW_ENGINE | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog statement mode and row engine |
| 1666 | ER_BINLOG_ROW_INJECTION_AND_STMT_MODE | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog row injection and statement mode |
| 1667 | ER_BINLOG_MULTIPLE_ENGINES_AND_SELF_LOGGING_ENGINE | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog multiple engines and self logging engine |
| 1668 | ER_BINLOG_UNSAFE_LIMIT | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe limit |
| 1670 | ER_BINLOG_UNSAFE_SYSTEM_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe system table |
| 1671 | ER_BINLOG_UNSAFE_AUTOINC_COLUMNS | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe autoinc columns |
| 1672 | ER_BINLOG_UNSAFE_UDF | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe UDF |
| 1673 | ER_BINLOG_UNSAFE_SYSTEM_VARIABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe system variable |
| 1674 | ER_BINLOG_UNSAFE_SYSTEM_FUNCTION | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe system function |
| 1675 | ER_BINLOG_UNSAFE_NONTRANS_AFTER_TRANS | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe nontrans after trans |
| 1676 | ER_MESSAGE_AND_STATEMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | message and statement |
| 1678 | ER_REPLICA_CANT_CREATE_CONVERSION | DatabaseError | 8.4: HY000, 9.7: HY000 | replica cannot create conversion |
| 1679 | ER_INSIDE_TRANSACTION_PREVENTS_SWITCH_BINLOG_FORMAT | DatabaseError | 8.4: HY000, 9.7: HY000 | inside transaction prevents switch binlog format |
| 1680 | ER_PATH_LENGTH | DatabaseError | 8.4: HY000, 9.7: HY000 | path length |
| 1681 | ER_WARN_DEPRECATED_SYNTAX_NO_REPLACEMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated syntax no replacement |
| 1682 | ER_WRONG_NATIVE_TABLE_STRUCTURE | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid native table structure |
| 1683 | ER_WRONG_PERFSCHEMA_USAGE | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid perfschema usage |
| 1684 | ER_WARN_I_S_SKIPPED_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | warn i s skipped table |
| 1685 | ER_INSIDE_TRANSACTION_PREVENTS_SWITCH_BINLOG_DIRECT | DatabaseError | 8.4: HY000, 9.7: HY000 | inside transaction prevents switch binlog direct |
| 1686 | ER_STORED_FUNCTION_PREVENTS_SWITCH_BINLOG_DIRECT | DatabaseError | 8.4: HY000, 9.7: HY000 | stored function prevents switch binlog direct |
| 1687 | ER_SPATIAL_MUST_HAVE_GEOM_COL | ProgrammingError | 8.4: 42000, 9.7: 42000 | spatial must have geom column |
| 1688 | ER_TOO_LONG_INDEX_COMMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | too long index comment |
| 1689 | ER_LOCK_ABORTED | DatabaseError | 8.4: HY000, 9.7: HY000 | lock aborted |
| 1690 | ER_DATA_OUT_OF_RANGE | DataError | 8.4: 22003, 9.7: 22003 | data out of range |
| 1692 | ER_BINLOG_UNSAFE_MULTIPLE_ENGINES_AND_SELF_LOGGING_ENGINE | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe multiple engines and self logging engine |
| 1693 | ER_BINLOG_UNSAFE_MIXED_STATEMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe mixed statement |
| 1694 | ER_INSIDE_TRANSACTION_PREVENTS_SWITCH_SQL_LOG_BIN | DatabaseError | 8.4: HY000, 9.7: HY000 | inside transaction prevents switch SQL log bin |
| 1695 | ER_STORED_FUNCTION_PREVENTS_SWITCH_SQL_LOG_BIN | DatabaseError | 8.4: HY000, 9.7: HY000 | stored function prevents switch SQL log bin |
| 1696 | ER_FAILED_READ_FROM_PAR_FILE | DatabaseError | 8.4: HY000, 9.7: HY000 | failed read from par file |
| 1697 | ER_VALUES_IS_NOT_INT_TYPE_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | values is not int type error |
| 1698 | ER_ACCESS_DENIED_NO_PASSWORD_ERROR | OperationalError | 8.4: 28000, 9.7: 28000 | access denied no password error |
| 1701 | ER_TRUNCATE_ILLEGAL_FK | ProgrammingError | 8.4: 42000, 9.7: 42000 | truncate illegal foreign key |
| 1702 | ER_PLUGIN_IS_PERMANENT | DatabaseError | 8.4: HY000, 9.7: HY000 | plugin is permanent |
| 1703 | ER_REPLICA_HEARTBEAT_VALUE_OUT_OF_RANGE_MIN | DatabaseError | 8.4: HY000, 9.7: HY000 | replica heartbeat value out of range min |
| 1704 | ER_REPLICA_HEARTBEAT_VALUE_OUT_OF_RANGE_MAX | DatabaseError | 8.4: HY000, 9.7: HY000 | replica heartbeat value out of range max |
| 1705 | ER_STMT_CACHE_FULL | DatabaseError | 8.4: HY000, 9.7: HY000 | statement cache full |
| 1706 | ER_MULTI_UPDATE_KEY_CONFLICT | DatabaseError | 8.4: HY000, 9.7: HY000 | multi update key conflict |
| 1707 | ER_TABLE_NEEDS_REBUILD | DatabaseError | 8.4: HY000, 9.7: HY000 | table needs rebuild |
| 1708 | WARN_OPTION_BELOW_LIMIT | DatabaseError | 8.4: HY000, 9.7: HY000 | option below limit |
| 1709 | ER_INDEX_COLUMN_TOO_LONG | DatabaseError | 8.4: HY000, 9.7: HY000 | index column too long |
| 1710 | ER_ERROR_IN_TRIGGER_BODY | DatabaseError | 8.4: HY000, 9.7: HY000 | error in trigger body |
| 1711 | ER_ERROR_IN_UNKNOWN_TRIGGER_BODY | DatabaseError | 8.4: HY000, 9.7: HY000 | error in unknown trigger body |
| 1712 | ER_INDEX_CORRUPT | DatabaseError | 8.4: HY000, 9.7: HY000 | index corrupt |
| 1713 | ER_UNDO_RECORD_TOO_BIG | DatabaseError | 8.4: HY000, 9.7: HY000 | undo record too big |
| 1714 | ER_BINLOG_UNSAFE_INSERT_IGNORE_SELECT | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe insert ignore select |
| 1715 | ER_BINLOG_UNSAFE_INSERT_SELECT_UPDATE | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe insert select update |
| 1716 | ER_BINLOG_UNSAFE_REPLACE_SELECT | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe replace select |
| 1717 | ER_BINLOG_UNSAFE_CREATE_IGNORE_SELECT | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe create ignore select |
| 1718 | ER_BINLOG_UNSAFE_CREATE_REPLACE_SELECT | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe create replace select |
| 1719 | ER_BINLOG_UNSAFE_UPDATE_IGNORE | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe update ignore |
| 1720 | ER_PLUGIN_NO_UNINSTALL | DatabaseError | 8.4: HY000, 9.7: HY000 | plugin no uninstall |
| 1721 | ER_PLUGIN_NO_INSTALL | DatabaseError | 8.4: HY000, 9.7: HY000 | plugin no install |
| 1722 | ER_BINLOG_UNSAFE_WRITE_AUTOINC_SELECT | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe write autoinc select |
| 1723 | ER_BINLOG_UNSAFE_CREATE_SELECT_AUTOINC | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe create select autoinc |
| 1724 | ER_BINLOG_UNSAFE_INSERT_TWO_KEYS | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe insert two keys |
| 1725 | ER_TABLE_IN_FK_CHECK | DatabaseError | 8.4: HY000, 9.7: HY000 | table in foreign key check |
| 1726 | ER_UNSUPPORTED_ENGINE | DatabaseError | 8.4: HY000, 9.7: HY000 | unsupported engine |
| 1727 | ER_BINLOG_UNSAFE_AUTOINC_NOT_FIRST | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe autoinc not first |
| 1728 | ER_CANNOT_LOAD_FROM_TABLE_V2 | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot load from table v2 |
| 1729 | ER_SOURCE_DELAY_VALUE_OUT_OF_RANGE | DatabaseError | 8.4: HY000, 9.7: HY000 | source delay value out of range |
| 1730 | ER_ONLY_FD_AND_RBR_EVENTS_ALLOWED_IN_BINLOG_STATEMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | only fd and rbr events allowed in binlog statement |
| 1731 | ER_PARTITION_EXCHANGE_DIFFERENT_OPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | partition exchange different option |
| 1732 | ER_PARTITION_EXCHANGE_PART_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | partition exchange part table |
| 1733 | ER_PARTITION_EXCHANGE_TEMP_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | partition exchange temp table |
| 1734 | ER_PARTITION_INSTEAD_OF_SUBPARTITION | DatabaseError | 8.4: HY000, 9.7: HY000 | partition instead of subpartition |
| 1735 | ER_UNKNOWN_PARTITION | DatabaseError | 8.4: HY000, 9.7: HY000 | unknown partition |
| 1736 | ER_TABLES_DIFFERENT_METADATA | DatabaseError | 8.4: HY000, 9.7: HY000 | tables different metadata |
| 1737 | ER_ROW_DOES_NOT_MATCH_PARTITION | DatabaseError | 8.4: HY000, 9.7: HY000 | row does not match partition |
| 1738 | ER_BINLOG_CACHE_SIZE_GREATER_THAN_MAX | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog cache size greater than max |
| 1739 | ER_WARN_INDEX_NOT_APPLICABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | warn index not applicable |
| 1740 | ER_PARTITION_EXCHANGE_FOREIGN_KEY | DatabaseError | 8.4: HY000, 9.7: HY000 | partition exchange foreign key |
| 1742 | ER_RPL_INFO_DATA_TOO_LONG | DatabaseError | 8.4: HY000, 9.7: HY000 | rpl info data too long |
| 1745 | ER_BINLOG_STMT_CACHE_SIZE_GREATER_THAN_MAX | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog statement cache size greater than max |
| 1746 | ER_CANT_UPDATE_TABLE_IN_CREATE_TABLE_SELECT | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot update table in create table select |
| 1747 | ER_PARTITION_CLAUSE_ON_NONPARTITIONED | DatabaseError | 8.4: HY000, 9.7: HY000 | partition clause on nonpartitioned |
| 1748 | ER_ROW_DOES_NOT_MATCH_GIVEN_PARTITION_SET | DatabaseError | 8.4: HY000, 9.7: HY000 | row does not match given partition set |
| 1750 | ER_CHANGE_RPL_INFO_REPOSITORY_FAILURE | DatabaseError | 8.4: HY000, 9.7: HY000 | change rpl info repository failure |
| 1751 | ER_WARNING_NOT_COMPLETE_ROLLBACK_WITH_CREATED_TEMP_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | warning not complete rollback with created temp table |
| 1752 | ER_WARNING_NOT_COMPLETE_ROLLBACK_WITH_DROPPED_TEMP_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | warning not complete rollback with dropped temp table |
| 1753 | ER_MTA_FEATURE_IS_NOT_SUPPORTED | DatabaseError | 8.4: HY000, 9.7: HY000 | mta feature is not supported |
| 1754 | ER_MTA_UPDATED_DBS_GREATER_MAX | DatabaseError | 8.4: HY000, 9.7: HY000 | mta updated databases greater max |
| 1755 | ER_MTA_CANT_PARALLEL | DatabaseError | 8.4: HY000, 9.7: HY000 | mta cannot parallel |
| 1756 | ER_MTA_INCONSISTENT_DATA | DatabaseError | 8.4: HY000, 9.7: HY000 | mta inconsistent data |
| 1757 | ER_FULLTEXT_NOT_SUPPORTED_WITH_PARTITIONING | DatabaseError | 8.4: HY000, 9.7: HY000 | fulltext not supported with partitioning |
| 1758 | ER_DA_INVALID_CONDITION_NUMBER | DatabaseError | 8.4: 35000, 9.7: 35000 | da invalid condition number |
| 1759 | ER_INSECURE_PLAIN_TEXT | DatabaseError | 8.4: HY000, 9.7: HY000 | insecure plain text |
| 1760 | ER_INSECURE_CHANGE_SOURCE | DatabaseError | 8.4: HY000, 9.7: HY000 | insecure change source |
| 1761 | ER_FOREIGN_DUPLICATE_KEY_WITH_CHILD_INFO | IntegrityError | 8.4: 23000, 9.7: 23000 | foreign duplicate key with child info |
| 1762 | ER_FOREIGN_DUPLICATE_KEY_WITHOUT_CHILD_INFO | IntegrityError | 8.4: 23000, 9.7: 23000 | foreign duplicate key without child info |
| 1763 | ER_SQLTHREAD_WITH_SECURE_REPLICA | DatabaseError | 8.4: HY000, 9.7: HY000 | sqlthread with secure replica |
| 1764 | ER_TABLE_HAS_NO_FT | DatabaseError | 8.4: HY000, 9.7: HY000 | table has no ft |
| 1765 | ER_VARIABLE_NOT_SETTABLE_IN_SF_OR_TRIGGER | DatabaseError | 8.4: HY000, 9.7: HY000 | variable not settable in sf or trigger |
| 1766 | ER_VARIABLE_NOT_SETTABLE_IN_TRANSACTION | DatabaseError | 8.4: HY000, 9.7: HY000 | variable not settable in transaction |
| 1769 | ER_SET_STATEMENT_CANNOT_INVOKE_FUNCTION | DatabaseError | 8.4: HY000, 9.7: HY000 | set statement cannot invoke function |
| 1770 | ER_GTID_NEXT_CANT_BE_AUTOMATIC_IF_GTID_NEXT_LIST_IS_NON_NULL | DatabaseError | 8.4: HY000, 9.7: HY000 | GTID next cannot be automatic if GTID next list is non null |
| 1772 | ER_MALFORMED_GTID_SET_SPECIFICATION | DatabaseError | 8.4: HY000, 9.7: HY000 | malformed GTID set specification |
| 1773 | ER_MALFORMED_GTID_SET_ENCODING | DatabaseError | 8.4: HY000, 9.7: HY000 | malformed GTID set encoding |
| 1774 | ER_MALFORMED_GTID_SPECIFICATION | DatabaseError | 8.4: HY000, 9.7: HY000 | malformed GTID specification |
| 1775 | ER_GNO_EXHAUSTED | DatabaseError | 8.4: HY000, 9.7: HY000 | gno exhausted |
| 1776 | ER_BAD_REPLICA_AUTO_POSITION | DatabaseError | 8.4: HY000, 9.7: HY000 | bad replica auto position |
| 1777 | ER_AUTO_POSITION_REQUIRES_GTID_MODE_NOT_OFF | DatabaseError | 8.4: HY000, 9.7: HY000 | auto position requires GTID mode not off |
| 1778 | ER_CANT_DO_IMPLICIT_COMMIT_IN_TRX_WHEN_GTID_NEXT_IS_SET | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot do implicit commit in transaction when GTID next is set |
| 1779 | ER_GTID_MODE_ON_REQUIRES_ENFORCE_GTID_CONSISTENCY_ON | DatabaseError | 8.4: HY000, 9.7: HY000 | GTID mode on requires enforce GTID consistency on |
| 1781 | ER_CANT_SET_GTID_NEXT_TO_GTID_WHEN_GTID_MODE_IS_OFF | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot set GTID next to GTID when GTID mode is off |
| 1782 | ER_CANT_SET_GTID_NEXT_TO_ANONYMOUS_WHEN_GTID_MODE_IS_ON | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot set GTID next to anonymous when GTID mode is on |
| 1783 | ER_CANT_SET_GTID_NEXT_LIST_TO_NON_NULL_WHEN_GTID_MODE_IS_OFF | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot set GTID next list to non null when GTID mode is off |
| 1785 | ER_GTID_UNSAFE_NON_TRANSACTIONAL_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | GTID unsafe non transactional table |
| 1786 | ER_GTID_UNSAFE_CREATE_SELECT | DatabaseError | 8.4: HY000, 9.7: HY000 | GTID unsafe create select |
| 1788 | ER_GTID_MODE_CAN_ONLY_CHANGE_ONE_STEP_AT_A_TIME | DatabaseError | 8.4: HY000, 9.7: HY000 | GTID mode can only change one step at a time |
| 1789 | ER_SOURCE_HAS_PURGED_REQUIRED_GTIDS | DatabaseError | 8.4: HY000, 9.7: HY000 | source has purged required gtids |
| 1790 | ER_CANT_SET_GTID_NEXT_WHEN_OWNING_GTID | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot set GTID next when owning GTID |
| 1791 | ER_UNKNOWN_EXPLAIN_FORMAT | DatabaseError | 8.4: HY000, 9.7: HY000 | unknown explain format |
| 1792 | ER_CANT_EXECUTE_IN_READ_ONLY_TRANSACTION | ProgrammingError | 8.4: 25006, 9.7: 25006 | cannot execute in read only transaction |
| 1793 | ER_TOO_LONG_TABLE_PARTITION_COMMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | too long table partition comment |
| 1794 | ER_REPLICA_CONFIGURATION | DatabaseError | 8.4: HY000, 9.7: HY000 | replica configuration |
| 1795 | ER_INNODB_FT_LIMIT | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb ft limit |
| 1796 | ER_INNODB_NO_FT_TEMP_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb no ft temp table |
| 1797 | ER_INNODB_FT_WRONG_DOCID_COLUMN | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb ft invalid docid column |
| 1798 | ER_INNODB_FT_WRONG_DOCID_INDEX | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb ft invalid docid index |
| 1799 | ER_INNODB_ONLINE_LOG_TOO_BIG | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb online log too big |
| 1800 | ER_UNKNOWN_ALTER_ALGORITHM | DatabaseError | 8.4: HY000, 9.7: HY000 | unknown alter algorithm |
| 1801 | ER_UNKNOWN_ALTER_LOCK | DatabaseError | 8.4: HY000, 9.7: HY000 | unknown alter lock |
| 1802 | ER_MTA_CHANGE_SOURCE_CANT_RUN_WITH_GAPS | DatabaseError | 8.4: HY000, 9.7: HY000 | mta change source cannot run with gaps |
| 1803 | ER_MTA_RECOVERY_FAILURE | DatabaseError | 8.4: HY000, 9.7: HY000 | mta recovery failure |
| 1804 | ER_MTA_RESET_WORKERS | DatabaseError | 8.4: HY000, 9.7: HY000 | mta reset workers |
| 1805 | ER_COL_COUNT_DOESNT_MATCH_CORRUPTED_V2 | DatabaseError | 8.4: HY000, 9.7: HY000 | column count doesnt match corrupted v2 |
| 1806 | ER_REPLICA_SILENT_RETRY_TRANSACTION | DatabaseError | 8.4: HY000, 9.7: HY000 | replica silent retry transaction |
| 1807 | ER_DISCARD_FK_CHECKS_RUNNING | DatabaseError | 8.4: HY000, 9.7: HY000 | discard foreign key checks running |
| 1808 | ER_TABLE_SCHEMA_MISMATCH | DatabaseError | 8.4: HY000, 9.7: HY000 | table schema mismatch |
| 1809 | ER_TABLE_IN_SYSTEM_TABLESPACE | DatabaseError | 8.4: HY000, 9.7: HY000 | table in system tablespace |
| 1810 | ER_IO_READ_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | I/O read error |
| 1811 | ER_IO_WRITE_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | I/O write error |
| 1812 | ER_TABLESPACE_MISSING | DatabaseError | 8.4: HY000, 9.7: HY000 | tablespace missing |
| 1813 | ER_TABLESPACE_EXISTS | DatabaseError | 8.4: HY000, 9.7: HY000 | tablespace exists |
| 1814 | ER_TABLESPACE_DISCARDED | DatabaseError | 8.4: HY000, 9.7: HY000 | tablespace discarded |
| 1815 | ER_INTERNAL_ERROR | InternalError | 8.4: HY000, 9.7: HY000 | internal error |
| 1816 | ER_INNODB_IMPORT_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb import error |
| 1817 | ER_INNODB_INDEX_CORRUPT | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb index corrupt |
| 1818 | ER_INVALID_YEAR_COLUMN_LENGTH | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid year column length |
| 1819 | ER_NOT_VALID_PASSWORD | DatabaseError | 8.4: HY000, 9.7: HY000 | not valid password |
| 1820 | ER_MUST_CHANGE_PASSWORD | DatabaseError | 8.4: HY000, 9.7: HY000 | must change password |
| 1821 | ER_FK_NO_INDEX_CHILD | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign key no index child |
| 1822 | ER_FK_NO_INDEX_PARENT | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign key no index parent |
| 1823 | ER_FK_FAIL_ADD_SYSTEM | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign key fail add system |
| 1824 | ER_FK_CANNOT_OPEN_PARENT | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign key cannot open parent |
| 1825 | ER_FK_INCORRECT_OPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign key incorrect option |
| 1826 | ER_FK_DUP_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign key dup name |
| 1827 | ER_PASSWORD_FORMAT | DatabaseError | 8.4: HY000, 9.7: HY000 | password format |
| 1828 | ER_FK_COLUMN_CANNOT_DROP | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign key column cannot drop |
| 1829 | ER_FK_COLUMN_CANNOT_DROP_CHILD | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign key column cannot drop child |
| 1830 | ER_FK_COLUMN_NOT_NULL | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign key column not null |
| 1831 | ER_DUP_INDEX | DatabaseError | 8.4: HY000, 9.7: HY000 | dup index |
| 1832 | ER_FK_COLUMN_CANNOT_CHANGE | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign key column cannot change |
| 1833 | ER_FK_COLUMN_CANNOT_CHANGE_CHILD | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign key column cannot change child |
| 1835 | ER_MALFORMED_PACKET | DatabaseError | 8.4: HY000, 9.7: HY000 | malformed packet |
| 1836 | ER_READ_ONLY_MODE | DatabaseError | 8.4: HY000, 9.7: HY000 | read only mode |
| 1837 | ER_GTID_NEXT_TYPE_UNDEFINED_GTID | DatabaseError | 8.4: HY000, 9.7: HY000 | GTID next type undefined GTID |
| 1838 | ER_VARIABLE_NOT_SETTABLE_IN_SP | DatabaseError | 8.4: HY000, 9.7: HY000 | variable not settable in sp |
| 1840 | ER_CANT_SET_GTID_PURGED_WHEN_GTID_EXECUTED_IS_NOT_EMPTY | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot set GTID purged when GTID executed is not empty |
| 1841 | ER_CANT_SET_GTID_PURGED_WHEN_OWNED_GTIDS_IS_NOT_EMPTY | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot set GTID purged when owned gtids is not empty |
| 1842 | ER_GTID_PURGED_WAS_CHANGED | DatabaseError | 8.4: HY000, 9.7: HY000 | GTID purged was changed |
| 1843 | ER_GTID_EXECUTED_WAS_CHANGED | DatabaseError | 8.4: HY000, 9.7: HY000 | GTID executed was changed |
| 1844 | ER_BINLOG_STMT_MODE_AND_NO_REPL_TABLES | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog statement mode and no repl tables |
| 1845 | ER_ALTER_OPERATION_NOT_SUPPORTED | NotSupportedError | 8.4: 0A000, 9.7: 0A000 | alter operation not supported |
| 1846 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON | NotSupportedError | 8.4: 0A000, 9.7: 0A000 | alter operation not supported reason |
| 1847 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_COPY | DatabaseError | 8.4: HY000, 9.7: HY000 | alter operation not supported reason copy |
| 1848 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_PARTITION | DatabaseError | 8.4: HY000, 9.7: HY000 | alter operation not supported reason partition |
| 1849 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_FK_RENAME | DatabaseError | 8.4: HY000, 9.7: HY000 | alter operation not supported reason foreign key rename |
| 1850 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_COLUMN_TYPE | DatabaseError | 8.4: HY000, 9.7: HY000 | alter operation not supported reason column type |
| 1851 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_FK_CHECK | DatabaseError | 8.4: HY000, 9.7: HY000 | alter operation not supported reason foreign key check |
| 1853 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_NOPK | DatabaseError | 8.4: HY000, 9.7: HY000 | alter operation not supported reason nopk |
| 1854 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_AUTOINC | DatabaseError | 8.4: HY000, 9.7: HY000 | alter operation not supported reason autoinc |
| 1855 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_HIDDEN_FTS | DatabaseError | 8.4: HY000, 9.7: HY000 | alter operation not supported reason hidden fts |
| 1856 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_CHANGE_FTS | DatabaseError | 8.4: HY000, 9.7: HY000 | alter operation not supported reason change fts |
| 1857 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_FTS | DatabaseError | 8.4: HY000, 9.7: HY000 | alter operation not supported reason fts |
| 1859 | ER_DUP_UNKNOWN_IN_INDEX | IntegrityError | 8.4: 23000, 9.7: 23000 | dup unknown in index |
| 1860 | ER_IDENT_CAUSES_TOO_LONG_PATH | DatabaseError | 8.4: HY000, 9.7: HY000 | ident causes too long path |
| 1861 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_NOT_NULL | DatabaseError | 8.4: HY000, 9.7: HY000 | alter operation not supported reason not null |
| 1862 | ER_MUST_CHANGE_PASSWORD_LOGIN | DatabaseError | 8.4: HY000, 9.7: HY000 | must change password login |
| 1863 | ER_ROW_IN_WRONG_PARTITION | DatabaseError | 8.4: HY000, 9.7: HY000 | row in invalid partition |
| 1864 | ER_MTA_EVENT_BIGGER_PENDING_JOBS_SIZE_MAX | DatabaseError | 8.4: HY000, 9.7: HY000 | mta event bigger pending jobs size max |
| 1866 | ER_BINLOG_LOGICAL_CORRUPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog logical corruption |
| 1867 | ER_WARN_PURGE_LOG_IN_USE | DatabaseError | 8.4: HY000, 9.7: HY000 | warn purge log in use |
| 1868 | ER_WARN_PURGE_LOG_IS_ACTIVE | DatabaseError | 8.4: HY000, 9.7: HY000 | warn purge log is active |
| 1869 | ER_AUTO_INCREMENT_CONFLICT | DatabaseError | 8.4: HY000, 9.7: HY000 | auto increment conflict |
| 1870 | WARN_ON_BLOCKHOLE_IN_RBR | DatabaseError | 8.4: HY000, 9.7: HY000 | on blockhole in rbr |
| 1871 | ER_REPLICA_CM_INIT_REPOSITORY | DatabaseError | 8.4: HY000, 9.7: HY000 | replica cm init repository |
| 1872 | ER_REPLICA_AM_INIT_REPOSITORY | DatabaseError | 8.4: HY000, 9.7: HY000 | replica am init repository |
| 1873 | ER_ACCESS_DENIED_CHANGE_USER_ERROR | OperationalError | 8.4: 28000, 9.7: 28000 | access denied change user error |
| 1874 | ER_INNODB_READ_ONLY | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb read only |
| 1875 | ER_STOP_REPLICA_SQL_THREAD_TIMEOUT | DatabaseError | 8.4: HY000, 9.7: HY000 | stop replica SQL thread timeout |
| 1876 | ER_STOP_REPLICA_IO_THREAD_TIMEOUT | DatabaseError | 8.4: HY000, 9.7: HY000 | stop replica I/O thread timeout |
| 1877 | ER_TABLE_CORRUPT | DatabaseError | 8.4: HY000, 9.7: HY000 | table corrupt |
| 1878 | ER_TEMP_FILE_WRITE_FAILURE | DatabaseError | 8.4: HY000, 9.7: HY000 | temp file write failure |
| 1879 | ER_INNODB_FT_AUX_NOT_HEX_ID | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb ft aux not hex id |
| 1880 | ER_OLD_TEMPORALS_UPGRADED | DatabaseError | 8.4: HY000, 9.7: HY000 | old temporals upgraded |
| 1881 | ER_INNODB_FORCED_RECOVERY | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb forced recovery |
| 1882 | ER_AES_INVALID_IV | DatabaseError | 8.4: HY000, 9.7: HY000 | aes invalid iv |
| 1883 | ER_PLUGIN_CANNOT_BE_UNINSTALLED | DatabaseError | 8.4: HY000, 9.7: HY000 | plugin cannot be uninstalled |
| 1884 | ER_GTID_UNSAFE_BINLOG_SPLITTABLE_STATEMENT_AND_ASSIGNED_GTID | DatabaseError | 8.4: HY000, 9.7: HY000 | GTID unsafe binlog splittable statement and assigned GTID |
| 1885 | ER_REPLICA_HAS_MORE_GTIDS_THAN_SOURCE | DatabaseError | 8.4: HY000, 9.7: HY000 | replica has more gtids than source |
| 1886 | ER_MISSING_KEY | DatabaseError | 8.4: HY000, 9.7: HY000 | missing key |
| 1887 | WARN_NAMED_PIPE_ACCESS_EVERYONE | DatabaseError | 8.4: HY000, 9.7: HY000 | named pipe access everyone |
| 3000 | ER_FILE_CORRUPT | DatabaseError | 8.4: HY000, 9.7: HY000 | file corrupt |
| 3001 | ER_ERROR_ON_SOURCE | DatabaseError | 8.4: HY000, 9.7: HY000 | error on source |
| 3003 | ER_STORAGE_ENGINE_NOT_LOADED | DatabaseError | 8.4: HY000, 9.7: HY000 | storage engine not loaded |
| 3004 | ER_GET_STACKED_DA_WITHOUT_ACTIVE_HANDLER | DatabaseError | 8.4: 0Z002, 9.7: 0Z002 | get stacked da without active handler |
| 3005 | ER_WARN_LEGACY_SYNTAX_CONVERTED | DatabaseError | 8.4: HY000, 9.7: HY000 | warn legacy syntax converted |
| 3006 | ER_BINLOG_UNSAFE_FULLTEXT_PLUGIN | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe fulltext plugin |
| 3007 | ER_CANNOT_DISCARD_TEMPORARY_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot discard temporary table |
| 3008 | ER_FK_DEPTH_EXCEEDED | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign key depth exceeded |
| 3009 | ER_COL_COUNT_DOESNT_MATCH_PLEASE_UPDATE_V2 | DatabaseError | 8.4: HY000, 9.7: HY000 | column count doesnt match please update v2 |
| 3010 | ER_WARN_TRIGGER_DOESNT_HAVE_CREATED | DatabaseError | 8.4: HY000, 9.7: HY000 | warn trigger doesnt have created |
| 3011 | ER_REFERENCED_TRG_DOES_NOT_EXIST | DatabaseError | 8.4: HY000, 9.7: HY000 | referenced trg does not exist |
| 3012 | ER_EXPLAIN_NOT_SUPPORTED | DatabaseError | 8.4: HY000, 9.7: HY000 | explain not supported |
| 3013 | ER_INVALID_FIELD_SIZE | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid field size |
| 3014 | ER_MISSING_HA_CREATE_OPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | missing ha create option |
| 3015 | ER_ENGINE_OUT_OF_MEMORY | DatabaseError | 8.4: HY000, 9.7: HY000 | engine out of memory |
| 3016 | ER_PASSWORD_EXPIRE_ANONYMOUS_USER | DatabaseError | 8.4: HY000, 9.7: HY000 | password expire anonymous user |
| 3017 | ER_REPLICA_SQL_THREAD_MUST_STOP | DatabaseError | 8.4: HY000, 9.7: HY000 | replica SQL thread must stop |
| 3018 | ER_NO_FT_MATERIALIZED_SUBQUERY | DatabaseError | 8.4: HY000, 9.7: HY000 | no ft materialized subquery |
| 3019 | ER_INNODB_UNDO_LOG_FULL | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb undo log full |
| 3020 | ER_INVALID_ARGUMENT_FOR_LOGARITHM | DataError | 8.4: 2201E, 9.7: 2201E | invalid argument for logarithm |
| 3021 | ER_REPLICA_CHANNEL_IO_THREAD_MUST_STOP | DatabaseError | 8.4: HY000, 9.7: HY000 | replica channel I/O thread must stop |
| 3022 | ER_WARN_OPEN_TEMP_TABLES_MUST_BE_ZERO | DatabaseError | 8.4: HY000, 9.7: HY000 | warn open temp tables must be zero |
| 3023 | ER_WARN_ONLY_SOURCE_LOG_FILE_NO_POS | DatabaseError | 8.4: HY000, 9.7: HY000 | warn only source log file no pos |
| 3024 | ER_QUERY_TIMEOUT | OperationalError | 8.4: HY000, 9.7: HY000 | query timeout |
| 3025 | ER_NON_RO_SELECT_DISABLE_TIMER | DatabaseError | 8.4: HY000, 9.7: HY000 | non ro select disable timer |
| 3026 | ER_DUP_LIST_ENTRY | DatabaseError | 8.4: HY000, 9.7: HY000 | dup list entry |
| 3028 | ER_AGGREGATE_ORDER_FOR_UNION | DatabaseError | 8.4: HY000, 9.7: HY000 | aggregate order for union |
| 3029 | ER_AGGREGATE_ORDER_NON_AGG_QUERY | DatabaseError | 8.4: HY000, 9.7: HY000 | aggregate order non agg query |
| 3030 | ER_REPLICA_WORKER_STOPPED_PREVIOUS_THD_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | replica worker stopped previous thd error |
| 3031 | ER_DONT_SUPPORT_REPLICA_PRESERVE_COMMIT_ORDER | DatabaseError | 8.4: HY000 | dont support replica preserve commit order |
| 3032 | ER_SERVER_OFFLINE_MODE | DatabaseError | 8.4: HY000, 9.7: HY000 | server offline mode |
| 3033 | ER_GIS_DIFFERENT_SRIDS | DatabaseError | 8.4: HY000, 9.7: HY000 | gis different srids |
| 3034 | ER_GIS_UNSUPPORTED_ARGUMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | gis unsupported argument |
| 3035 | ER_GIS_UNKNOWN_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | gis unknown error |
| 3036 | ER_GIS_UNKNOWN_EXCEPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | gis unknown exception |
| 3037 | ER_GIS_INVALID_DATA | DataError | 8.4: 22023, 9.7: 22023 | gis invalid data |
| 3038 | ER_BOOST_GEOMETRY_EMPTY_INPUT_EXCEPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | boost geometry empty input exception |
| 3039 | ER_BOOST_GEOMETRY_CENTROID_EXCEPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | boost geometry centroid exception |
| 3040 | ER_BOOST_GEOMETRY_OVERLAY_INVALID_INPUT_EXCEPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | boost geometry overlay invalid input exception |
| 3041 | ER_BOOST_GEOMETRY_TURN_INFO_EXCEPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | boost geometry turn info exception |
| 3042 | ER_BOOST_GEOMETRY_SELF_INTERSECTION_POINT_EXCEPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | boost geometry self intersection point exception |
| 3043 | ER_BOOST_GEOMETRY_UNKNOWN_EXCEPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | boost geometry unknown exception |
| 3044 | ER_STD_BAD_ALLOC_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | std bad alloc error |
| 3045 | ER_STD_DOMAIN_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | std domain error |
| 3046 | ER_STD_LENGTH_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | std length error |
| 3047 | ER_STD_INVALID_ARGUMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | std invalid argument |
| 3048 | ER_STD_OUT_OF_RANGE_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | std out of range error |
| 3049 | ER_STD_OVERFLOW_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | std overflow error |
| 3050 | ER_STD_RANGE_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | std range error |
| 3051 | ER_STD_UNDERFLOW_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | std underflow error |
| 3052 | ER_STD_LOGIC_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | std logic error |
| 3053 | ER_STD_RUNTIME_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | std runtime error |
| 3054 | ER_STD_UNKNOWN_EXCEPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | std unknown exception |
| 3055 | ER_GIS_DATA_WRONG_ENDIANESS | DatabaseError | 8.4: HY000, 9.7: HY000 | gis data invalid endianess |
| 3056 | ER_CHANGE_SOURCE_PASSWORD_LENGTH | DatabaseError | 8.4: HY000, 9.7: HY000 | change source password length |
| 3057 | ER_USER_LOCK_WRONG_NAME | ProgrammingError | 8.4: 42000, 9.7: 42000 | user lock invalid name |
| 3058 | ER_USER_LOCK_DEADLOCK | DatabaseError | 8.4: HY000, 9.7: HY000 | user lock deadlock |
| 3059 | ER_REPLACE_INACCESSIBLE_ROWS | DatabaseError | 8.4: HY000, 9.7: HY000 | replace inaccessible rows |
| 3060 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_GIS | DatabaseError | 8.4: HY000, 9.7: HY000 | alter operation not supported reason gis |
| 3061 | ER_ILLEGAL_USER_VAR | ProgrammingError | 8.4: 42000, 9.7: 42000 | illegal user variable |
| 3062 | ER_GTID_MODE_OFF | DatabaseError | 8.4: HY000, 9.7: HY000 | GTID mode off |
| 3064 | ER_INCORRECT_TYPE | DatabaseError | 8.4: HY000, 9.7: HY000 | incorrect type |
| 3065 | ER_FIELD_IN_ORDER_NOT_SELECT | DatabaseError | 8.4: HY000, 9.7: HY000 | field in order not select |
| 3066 | ER_AGGREGATE_IN_ORDER_NOT_SELECT | DatabaseError | 8.4: HY000, 9.7: HY000 | aggregate in order not select |
| 3067 | ER_INVALID_RPL_WILD_TABLE_FILTER_PATTERN | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid rpl wild table filter pattern |
| 3068 | ER_NET_OK_PACKET_TOO_LARGE | OperationalError | 8.4: 08S01, 9.7: 08S01 | net ok packet too large |
| 3069 | ER_INVALID_JSON_DATA | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid JSON data |
| 3070 | ER_INVALID_GEOJSON_MISSING_MEMBER | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid geojson missing member |
| 3071 | ER_INVALID_GEOJSON_WRONG_TYPE | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid geojson invalid type |
| 3072 | ER_INVALID_GEOJSON_UNSPECIFIED | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid geojson unspecified |
| 3073 | ER_DIMENSION_UNSUPPORTED | DatabaseError | 8.4: HY000, 9.7: HY000 | dimension unsupported |
| 3074 | ER_REPLICA_CHANNEL_DOES_NOT_EXIST | DatabaseError | 8.4: HY000, 9.7: HY000 | replica channel does not exist |
| 3076 | ER_REPLICA_CHANNEL_NAME_INVALID_OR_TOO_LONG | DatabaseError | 8.4: HY000, 9.7: HY000 | replica channel name invalid or too long |
| 3077 | ER_REPLICA_NEW_CHANNEL_WRONG_REPOSITORY | DatabaseError | 8.4: HY000, 9.7: HY000 | replica new channel invalid repository |
| 3079 | ER_REPLICA_MULTIPLE_CHANNELS_CMD | DatabaseError | 8.4: HY000, 9.7: HY000 | replica multiple channels cmd |
| 3080 | ER_REPLICA_MAX_CHANNELS_EXCEEDED | DatabaseError | 8.4: HY000, 9.7: HY000 | replica max channels exceeded |
| 3081 | ER_REPLICA_CHANNEL_MUST_STOP | DatabaseError | 8.4: HY000, 9.7: HY000 | replica channel must stop |
| 3082 | ER_REPLICA_CHANNEL_NOT_RUNNING | DatabaseError | 8.4: HY000, 9.7: HY000 | replica channel not running |
| 3083 | ER_REPLICA_CHANNEL_WAS_RUNNING | DatabaseError | 8.4: HY000, 9.7: HY000 | replica channel was running |
| 3084 | ER_REPLICA_CHANNEL_WAS_NOT_RUNNING | DatabaseError | 8.4: HY000, 9.7: HY000 | replica channel was not running |
| 3085 | ER_REPLICA_CHANNEL_SQL_THREAD_MUST_STOP | DatabaseError | 8.4: HY000, 9.7: HY000 | replica channel SQL thread must stop |
| 3086 | ER_REPLICA_CHANNEL_SQL_SKIP_COUNTER | DatabaseError | 8.4: HY000, 9.7: HY000 | replica channel SQL skip counter |
| 3087 | ER_WRONG_FIELD_WITH_GROUP_V2 | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid field with group v2 |
| 3088 | ER_MIX_OF_GROUP_FUNC_AND_FIELDS_V2 | DatabaseError | 8.4: HY000, 9.7: HY000 | mix of group function and fields v2 |
| 3089 | ER_WARN_DEPRECATED_SYSVAR_UPDATE | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated sysvar update |
| 3090 | ER_WARN_DEPRECATED_SQLMODE | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated sqlmode |
| 3091 | ER_CANNOT_LOG_PARTIAL_DROP_DATABASE_WITH_GTID | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot log partial drop database with GTID |
| 3092 | ER_GROUP_REPLICATION_CONFIGURATION | DatabaseError | 8.4: HY000, 9.7: HY000 | group replication configuration |
| 3093 | ER_GROUP_REPLICATION_RUNNING | DatabaseError | 8.4: HY000, 9.7: HY000 | group replication running |
| 3094 | ER_GROUP_REPLICATION_APPLIER_INIT_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | group replication applier init error |
| 3095 | ER_GROUP_REPLICATION_STOP_APPLIER_THREAD_TIMEOUT | DatabaseError | 8.4: HY000, 9.7: HY000 | group replication stop applier thread timeout |
| 3096 | ER_GROUP_REPLICATION_COMMUNICATION_LAYER_SESSION_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | group replication communication layer session error |
| 3097 | ER_GROUP_REPLICATION_COMMUNICATION_LAYER_JOIN_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | group replication communication layer join error |
| 3098 | ER_BEFORE_DML_VALIDATION_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | before DML validation error |
| 3099 | ER_PREVENTS_VARIABLE_WITHOUT_RBR | DatabaseError | 8.4: HY000, 9.7: HY000 | prevents variable without rbr |
| 3100 | ER_RUN_HOOK_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | run hook error |
| 3101 | ER_TRANSACTION_ROLLBACK_DURING_COMMIT | OperationalError | 8.4: 40000, 9.7: 40000 | transaction rollback during commit |
| 3102 | ER_GENERATED_COLUMN_FUNCTION_IS_NOT_ALLOWED | DatabaseError | 8.4: HY000, 9.7: HY000 | generated column function is not allowed |
| 3103 | ER_UNSUPPORTED_ALTER_INPLACE_ON_VIRTUAL_COLUMN | DatabaseError | 8.4: HY000, 9.7: HY000 | unsupported alter inplace on virtual column |
| 3104 | ER_WRONG_FK_OPTION_FOR_GENERATED_COLUMN | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid foreign key option for generated column |
| 3105 | ER_NON_DEFAULT_VALUE_FOR_GENERATED_COLUMN | DatabaseError | 8.4: HY000, 9.7: HY000 | non default value for generated column |
| 3106 | ER_UNSUPPORTED_ACTION_ON_GENERATED_COLUMN | DatabaseError | 8.4: HY000, 9.7: HY000 | unsupported action on generated column |
| 3107 | ER_GENERATED_COLUMN_NON_PRIOR | DatabaseError | 8.4: HY000, 9.7: HY000 | generated column non prior |
| 3108 | ER_DEPENDENT_BY_GENERATED_COLUMN | DatabaseError | 8.4: HY000, 9.7: HY000 | dependent by generated column |
| 3109 | ER_GENERATED_COLUMN_REF_AUTO_INC | DatabaseError | 8.4: HY000, 9.7: HY000 | generated column reference auto inc |
| 3110 | ER_FEATURE_NOT_AVAILABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | feature not available |
| 3111 | ER_CANT_SET_GTID_MODE | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot set GTID mode |
| 3112 | ER_CANT_USE_AUTO_POSITION_WITH_GTID_MODE_OFF | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot use auto position with GTID mode off |
| 3116 | ER_CANT_ENFORCE_GTID_CONSISTENCY_WITH_ONGOING_GTID_VIOLATING_TX | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot enforce GTID consistency with ongoing GTID violating transaction |
| 3117 | ER_ENFORCE_GTID_CONSISTENCY_WARN_WITH_ONGOING_GTID_VIOLATING_TX | DatabaseError | 8.4: HY000, 9.7: HY000 | enforce GTID consistency warn with ongoing GTID violating transaction |
| 3118 | ER_ACCOUNT_HAS_BEEN_LOCKED | DatabaseError | 8.4: HY000, 9.7: HY000 | account has been locked |
| 3119 | ER_WRONG_TABLESPACE_NAME | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid tablespace name |
| 3120 | ER_TABLESPACE_IS_NOT_EMPTY | DatabaseError | 8.4: HY000, 9.7: HY000 | tablespace is not empty |
| 3121 | ER_WRONG_FILE_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid file name |
| 3122 | ER_BOOST_GEOMETRY_INCONSISTENT_TURNS_EXCEPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | boost geometry inconsistent turns exception |
| 3123 | ER_WARN_OPTIMIZER_HINT_SYNTAX_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | warn optimizer hint syntax error |
| 3124 | ER_WARN_BAD_MAX_EXECUTION_TIME | DatabaseError | 8.4: HY000, 9.7: HY000 | warn bad max execution time |
| 3125 | ER_WARN_UNSUPPORTED_MAX_EXECUTION_TIME | DatabaseError | 8.4: HY000, 9.7: HY000 | warn unsupported max execution time |
| 3126 | ER_WARN_CONFLICTING_HINT | DatabaseError | 8.4: HY000, 9.7: HY000 | warn conflicting hint |
| 3127 | ER_WARN_UNKNOWN_QB_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | warn unknown qb name |
| 3128 | ER_UNRESOLVED_HINT_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | unresolved hint name |
| 3129 | ER_WARN_ON_MODIFYING_GTID_EXECUTED_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | warn on modifying GTID executed table |
| 3130 | ER_PLUGGABLE_PROTOCOL_COMMAND_NOT_SUPPORTED | DatabaseError | 8.4: HY000, 9.7: HY000 | pluggable protocol command not supported |
| 3131 | ER_LOCKING_SERVICE_WRONG_NAME | ProgrammingError | 8.4: 42000, 9.7: 42000 | locking service invalid name |
| 3132 | ER_LOCKING_SERVICE_DEADLOCK | DatabaseError | 8.4: HY000, 9.7: HY000 | locking service deadlock |
| 3133 | ER_LOCKING_SERVICE_TIMEOUT | DatabaseError | 8.4: HY000, 9.7: HY000 | locking service timeout |
| 3134 | ER_GIS_MAX_POINTS_IN_GEOMETRY_OVERFLOWED | DatabaseError | 8.4: HY000, 9.7: HY000 | gis max points in geometry overflowed |
| 3135 | ER_SQL_MODE_MERGED | DatabaseError | 8.4: HY000, 9.7: HY000 | SQL mode merged |
| 3136 | ER_VTOKEN_PLUGIN_TOKEN_MISMATCH | DatabaseError | 8.4: HY000 | vtoken plugin token mismatch |
| 3137 | ER_VTOKEN_PLUGIN_TOKEN_NOT_FOUND | DatabaseError | 8.4: HY000 | vtoken plugin token not found |
| 3138 | ER_CANT_SET_VARIABLE_WHEN_OWNING_GTID | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot set variable when owning GTID |
| 3139 | ER_REPLICA_CHANNEL_OPERATION_NOT_ALLOWED | DatabaseError | 8.4: HY000, 9.7: HY000 | replica channel operation not allowed |
| 3140 | ER_INVALID_JSON_TEXT | DataError | 8.4: 22032, 9.7: 22032 | invalid JSON text |
| 3141 | ER_INVALID_JSON_TEXT_IN_PARAM | DataError | 8.4: 22032, 9.7: 22032 | invalid JSON text in param |
| 3142 | ER_INVALID_JSON_BINARY_DATA | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid JSON binary data |
| 3143 | ER_INVALID_JSON_PATH | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid JSON path |
| 3144 | ER_INVALID_JSON_CHARSET | DataError | 8.4: 22032, 9.7: 22032 | invalid JSON character set |
| 3145 | ER_INVALID_JSON_CHARSET_IN_FUNCTION | DataError | 8.4: 22032, 9.7: 22032 | invalid JSON character set in function |
| 3146 | ER_INVALID_TYPE_FOR_JSON | DataError | 8.4: 22032, 9.7: 22032 | invalid type for JSON |
| 3147 | ER_INVALID_CAST_TO_JSON | DataError | 8.4: 22032, 9.7: 22032 | invalid cast to JSON |
| 3148 | ER_INVALID_JSON_PATH_CHARSET | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid JSON path character set |
| 3149 | ER_INVALID_JSON_PATH_WILDCARD | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid JSON path wildcard |
| 3150 | ER_JSON_VALUE_TOO_BIG | DataError | 8.4: 22032, 9.7: 22032 | JSON value too big |
| 3151 | ER_JSON_KEY_TOO_BIG | DataError | 8.4: 22032, 9.7: 22032 | JSON key too big |
| 3152 | ER_JSON_USED_AS_KEY | ProgrammingError | 8.4: 42000, 9.7: 42000 | JSON used as key |
| 3153 | ER_JSON_VACUOUS_PATH | ProgrammingError | 8.4: 42000, 9.7: 42000 | JSON vacuous path |
| 3154 | ER_JSON_BAD_ONE_OR_ALL_ARG | ProgrammingError | 8.4: 42000, 9.7: 42000 | JSON bad one or all argument |
| 3155 | ER_NUMERIC_JSON_VALUE_OUT_OF_RANGE | DataError | 8.4: 22003, 9.7: 22003 | numeric JSON value out of range |
| 3156 | ER_INVALID_JSON_VALUE_FOR_CAST | DataError | 8.4: 22018, 9.7: 22018 | invalid JSON value for cast |
| 3157 | ER_JSON_DOCUMENT_TOO_DEEP | DataError | 8.4: 22032, 9.7: 22032 | JSON document too deep |
| 3158 | ER_JSON_DOCUMENT_NULL_KEY | DataError | 8.4: 22032, 9.7: 22032 | JSON document null key |
| 3159 | ER_SECURE_TRANSPORT_REQUIRED | DatabaseError | 8.4: HY000, 9.7: HY000 | secure transport required |
| 3160 | ER_NO_SECURE_TRANSPORTS_CONFIGURED | DatabaseError | 8.4: HY000, 9.7: HY000 | no secure transports configured |
| 3161 | ER_DISABLED_STORAGE_ENGINE | DatabaseError | 8.4: HY000, 9.7: HY000 | disabled storage engine |
| 3162 | ER_USER_DOES_NOT_EXIST | DatabaseError | 8.4: HY000, 9.7: HY000 | user does not exist |
| 3163 | ER_USER_ALREADY_EXISTS | DatabaseError | 8.4: HY000, 9.7: HY000 | user already exists |
| 3164 | ER_AUDIT_API_ABORT | DatabaseError | 8.4: HY000, 9.7: HY000 | audit api abort |
| 3165 | ER_INVALID_JSON_PATH_ARRAY_CELL | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid JSON path array cell |
| 3166 | ER_BUFPOOL_RESIZE_INPROGRESS | DatabaseError | 8.4: HY000, 9.7: HY000 | bufpool resize inprogress |
| 3167 | ER_FEATURE_DISABLED_SEE_DOC | DatabaseError | 8.4: HY000, 9.7: HY000 | feature disabled see doc |
| 3168 | ER_SERVER_ISNT_AVAILABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | server isnt available |
| 3169 | ER_SESSION_WAS_KILLED | OperationalError | 8.4: HY000, 9.7: HY000 | session was killed |
| 3170 | ER_CAPACITY_EXCEEDED | DatabaseError | 8.4: HY000, 9.7: HY000 | capacity exceeded |
| 3171 | ER_CAPACITY_EXCEEDED_IN_RANGE_OPTIMIZER | DatabaseError | 8.4: HY000, 9.7: HY000 | capacity exceeded in range optimizer |
| 3173 | ER_CANT_WAIT_FOR_EXECUTED_GTID_SET_WHILE_OWNING_A_GTID | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot wait for executed GTID set while owning a GTID |
| 3174 | ER_CANNOT_ADD_FOREIGN_BASE_COL_VIRTUAL | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot add foreign base column virtual |
| 3175 | ER_CANNOT_CREATE_VIRTUAL_INDEX_CONSTRAINT | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot create virtual index constraint |
| 3176 | ER_ERROR_ON_MODIFYING_GTID_EXECUTED_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | error on modifying GTID executed table |
| 3177 | ER_LOCK_REFUSED_BY_ENGINE | DatabaseError | 8.4: HY000, 9.7: HY000 | lock refused by engine |
| 3178 | ER_UNSUPPORTED_ALTER_ONLINE_ON_VIRTUAL_COLUMN | DatabaseError | 8.4: HY000, 9.7: HY000 | unsupported alter online on virtual column |
| 3179 | ER_MASTER_KEY_ROTATION_NOT_SUPPORTED_BY_SE | DatabaseError | 8.4: HY000, 9.7: HY000 | master key rotation not supported by se |
| 3181 | ER_MASTER_KEY_ROTATION_BINLOG_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | master key rotation binlog failed |
| 3182 | ER_MASTER_KEY_ROTATION_SE_UNAVAILABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | master key rotation se unavailable |
| 3183 | ER_TABLESPACE_CANNOT_ENCRYPT | DatabaseError | 8.4: HY000, 9.7: HY000 | tablespace cannot encrypt |
| 3184 | ER_INVALID_ENCRYPTION_OPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid encryption option |
| 3185 | ER_CANNOT_FIND_KEY_IN_KEYRING | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot find key in keyring |
| 3186 | ER_CAPACITY_EXCEEDED_IN_PARSER | DatabaseError | 8.4: HY000, 9.7: HY000 | capacity exceeded in parser |
| 3187 | ER_UNSUPPORTED_ALTER_ENCRYPTION_INPLACE | DatabaseError | 8.4: HY000, 9.7: HY000 | unsupported alter encryption inplace |
| 3188 | ER_KEYRING_UDF_KEYRING_SERVICE_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | keyring UDF keyring service error |
| 3189 | ER_USER_COLUMN_OLD_LENGTH | DatabaseError | 8.4: HY000, 9.7: HY000 | user column old length |
| 3190 | ER_CANT_RESET_SOURCE | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot reset source |
| 3191 | ER_GROUP_REPLICATION_MAX_GROUP_SIZE | DatabaseError | 8.4: HY000, 9.7: HY000 | group replication max group size |
| 3192 | ER_CANNOT_ADD_FOREIGN_BASE_COL_STORED | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot add foreign base column stored |
| 3193 | ER_TABLE_REFERENCED | DatabaseError | 8.4: HY000, 9.7: HY000 | table referenced |
| 3197 | ER_XA_RETRY | DatabaseError | 8.4: HY000, 9.7: HY000 | XA retry |
| 3198 | ER_KEYRING_AWS_UDF_AWS_KMS_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | keyring aws UDF aws kms error |
| 3199 | ER_BINLOG_UNSAFE_XA | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe XA |
| 3200 | ER_UDF_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | UDF error |
| 3201 | ER_KEYRING_MIGRATION_FAILURE | DatabaseError | 8.4: HY000, 9.7: HY000 | keyring migration failure |
| 3202 | ER_KEYRING_ACCESS_DENIED_ERROR | ProgrammingError | 8.4: 42000, 9.7: 42000 | keyring access denied error |
| 3203 | ER_KEYRING_MIGRATION_STATUS | DatabaseError | 8.4: HY000, 9.7: HY000 | keyring migration status |
| 3218 | ER_AUDIT_LOG_UDF_READ_INVALID_MAX_ARRAY_LENGTH_ARG_VALUE | DatabaseError | 8.4: HY000, 9.7: HY000 | audit log UDF read invalid max array length argument value |
| 3231 | ER_WRITE_SET_EXCEEDS_LIMIT | DatabaseError | 8.4: HY000, 9.7: HY000 | write set exceeds limit |
| 3235 | ER_AES_INVALID_KDF_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | aes invalid kdf name |
| 3236 | ER_AES_INVALID_KDF_ITERATIONS | DatabaseError | 8.4: HY000, 9.7: HY000 | aes invalid kdf iterations |
| 3237 | WARN_AES_KEY_SIZE | DatabaseError | 8.4: HY000, 9.7: HY000 | aes key size |
| 3238 | ER_AES_INVALID_KDF_OPTION_SIZE | DatabaseError | 8.4: HY000, 9.7: HY000 | aes invalid kdf option size |
| 3500 | ER_UNSUPPORT_COMPRESSED_TEMPORARY_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | unsupport compressed temporary table |
| 3501 | ER_ACL_OPERATION_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | acl operation failed |
| 3502 | ER_UNSUPPORTED_INDEX_ALGORITHM | DatabaseError | 8.4: HY000, 9.7: HY000 | unsupported index algorithm |
| 3503 | ER_NO_SUCH_DB | ProgrammingError | 8.4: 42Y07, 9.7: 42Y07 | no such database |
| 3504 | ER_TOO_BIG_ENUM | DatabaseError | 8.4: HY000, 9.7: HY000 | too big enum |
| 3505 | ER_TOO_LONG_SET_ENUM_VALUE | DatabaseError | 8.4: HY000, 9.7: HY000 | too long set enum value |
| 3506 | ER_INVALID_DD_OBJECT | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid dd object |
| 3507 | ER_UPDATING_DD_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | updating dd table |
| 3508 | ER_INVALID_DD_OBJECT_ID | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid dd object id |
| 3509 | ER_INVALID_DD_OBJECT_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid dd object name |
| 3510 | ER_TABLESPACE_MISSING_WITH_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | tablespace missing with name |
| 3511 | ER_TOO_LONG_ROUTINE_COMMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | too long routine comment |
| 3512 | ER_SP_LOAD_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | sp load failed |
| 3513 | ER_INVALID_BITWISE_OPERANDS_SIZE | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid bitwise operands size |
| 3514 | ER_INVALID_BITWISE_AGGREGATE_OPERANDS_SIZE | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid bitwise aggregate operands size |
| 3515 | ER_WARN_UNSUPPORTED_HINT | DatabaseError | 8.4: HY000, 9.7: HY000 | warn unsupported hint |
| 3516 | ER_UNEXPECTED_GEOMETRY_TYPE | DataError | 8.4: 22S01, 9.7: 22S01 | unexpected geometry type |
| 3517 | ER_SRS_PARSE_ERROR | DatabaseError | 8.4: SR002, 9.7: SR002 | spatial reference system parse error |
| 3518 | ER_SRS_PROJ_PARAMETER_MISSING | DatabaseError | 8.4: SR003, 9.7: SR003 | spatial reference system proj parameter missing |
| 3519 | ER_WARN_SRS_NOT_FOUND | DatabaseError | 8.4: 01000, 9.7: 01000 | warn spatial reference system not found |
| 3520 | ER_SRS_NOT_CARTESIAN | DataError | 8.4: 22S00, 9.7: 22S00 | spatial reference system not cartesian |
| 3521 | ER_SRS_NOT_CARTESIAN_UNDEFINED | DatabaseError | 8.4: SR001, 9.7: SR001 | spatial reference system not cartesian undefined |
| 3522 | ER_PK_INDEX_CANT_BE_INVISIBLE | DatabaseError | 8.4: HY000, 9.7: HY000 | primary key index cannot be invisible |
| 3523 | ER_UNKNOWN_AUTHID | DatabaseError | 8.4: HY000, 9.7: HY000 | unknown authid |
| 3524 | ER_FAILED_ROLE_GRANT | DatabaseError | 8.4: HY000, 9.7: HY000 | failed role grant |
| 3525 | ER_OPEN_ROLE_TABLES | DatabaseError | 8.4: HY000, 9.7: HY000 | open role tables |
| 3526 | ER_FAILED_DEFAULT_ROLES | DatabaseError | 8.4: HY000, 9.7: HY000 | failed default roles |
| 3527 | ER_COMPONENTS_NO_SCHEME | DatabaseError | 8.4: HY000, 9.7: HY000 | components no scheme |
| 3528 | ER_COMPONENTS_NO_SCHEME_SERVICE | DatabaseError | 8.4: HY000, 9.7: HY000 | components no scheme service |
| 3529 | ER_COMPONENTS_CANT_LOAD | DatabaseError | 8.4: HY000, 9.7: HY000 | components cannot load |
| 3530 | ER_ROLE_NOT_GRANTED | DatabaseError | 8.4: HY000, 9.7: HY000 | role not granted |
| 3531 | ER_FAILED_REVOKE_ROLE | DatabaseError | 8.4: HY000, 9.7: HY000 | failed revoke role |
| 3532 | ER_RENAME_ROLE | DatabaseError | 8.4: HY000, 9.7: HY000 | rename role |
| 3533 | ER_COMPONENTS_CANT_ACQUIRE_SERVICE_IMPLEMENTATION | DatabaseError | 8.4: HY000, 9.7: HY000 | components cannot acquire service implementation |
| 3534 | ER_COMPONENTS_CANT_SATISFY_DEPENDENCY | DatabaseError | 8.4: HY000, 9.7: HY000 | components cannot satisfy dependency |
| 3535 | ER_COMPONENTS_LOAD_CANT_REGISTER_SERVICE_IMPLEMENTATION | DatabaseError | 8.4: HY000, 9.7: HY000 | components load cannot register service implementation |
| 3536 | ER_COMPONENTS_LOAD_CANT_INITIALIZE | DatabaseError | 8.4: HY000, 9.7: HY000 | components load cannot initialize |
| 3537 | ER_COMPONENTS_UNLOAD_NOT_LOADED | DatabaseError | 8.4: HY000, 9.7: HY000 | components unload not loaded |
| 3538 | ER_COMPONENTS_UNLOAD_CANT_DEINITIALIZE | DatabaseError | 8.4: HY000, 9.7: HY000 | components unload cannot deinitialize |
| 3539 | ER_COMPONENTS_CANT_RELEASE_SERVICE | DatabaseError | 8.4: HY000, 9.7: HY000 | components cannot release service |
| 3540 | ER_COMPONENTS_UNLOAD_CANT_UNREGISTER_SERVICE | DatabaseError | 8.4: HY000, 9.7: HY000 | components unload cannot unregister service |
| 3541 | ER_COMPONENTS_CANT_UNLOAD | DatabaseError | 8.4: HY000, 9.7: HY000 | components cannot unload |
| 3542 | ER_WARN_UNLOAD_THE_NOT_PERSISTED | DatabaseError | 8.4: HY000, 9.7: HY000 | warn unload the not persisted |
| 3543 | ER_COMPONENT_TABLE_INCORRECT | DatabaseError | 8.4: HY000, 9.7: HY000 | component table incorrect |
| 3544 | ER_COMPONENT_MANIPULATE_ROW_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | component manipulate row failed |
| 3545 | ER_COMPONENTS_UNLOAD_DUPLICATE_IN_GROUP | DatabaseError | 8.4: HY000, 9.7: HY000 | components unload duplicate in group |
| 3546 | ER_CANT_SET_GTID_PURGED_DUE_SETS_CONSTRAINTS | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot set GTID purged due sets constraints |
| 3547 | ER_CANNOT_LOCK_USER_MANAGEMENT_CACHES | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot lock user management caches |
| 3548 | ER_SRS_NOT_FOUND | DatabaseError | 8.4: SR001, 9.7: SR001 | spatial reference system not found |
| 3549 | ER_VARIABLE_NOT_PERSISTED | DatabaseError | 8.4: HY000, 9.7: HY000 | variable not persisted |
| 3550 | ER_IS_QUERY_INVALID_CLAUSE | DatabaseError | 8.4: HY000, 9.7: HY000 | is query invalid clause |
| 3551 | ER_UNABLE_TO_STORE_STATISTICS | DatabaseError | 8.4: HY000, 9.7: HY000 | unable to store statistics |
| 3552 | ER_NO_SYSTEM_SCHEMA_ACCESS | DatabaseError | 8.4: HY000, 9.7: HY000 | no system schema access |
| 3553 | ER_NO_SYSTEM_TABLESPACE_ACCESS | DatabaseError | 8.4: HY000, 9.7: HY000 | no system tablespace access |
| 3554 | ER_NO_SYSTEM_TABLE_ACCESS | DatabaseError | 8.4: HY000, 9.7: HY000 | no system table access |
| 3555 | ER_NO_SYSTEM_TABLE_ACCESS_FOR_DICTIONARY_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | no system table access for dictionary table |
| 3556 | ER_NO_SYSTEM_TABLE_ACCESS_FOR_SYSTEM_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | no system table access for system table |
| 3557 | ER_NO_SYSTEM_TABLE_ACCESS_FOR_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | no system table access for table |
| 3558 | ER_INVALID_OPTION_KEY | DataError | 8.4: 22023, 9.7: 22023 | invalid option key |
| 3559 | ER_INVALID_OPTION_VALUE | DataError | 8.4: 22023, 9.7: 22023 | invalid option value |
| 3560 | ER_INVALID_OPTION_KEY_VALUE_PAIR | DataError | 8.4: 22023, 9.7: 22023 | invalid option key value pair |
| 3561 | ER_INVALID_OPTION_START_CHARACTER | DataError | 8.4: 22023, 9.7: 22023 | invalid option start character |
| 3562 | ER_INVALID_OPTION_END_CHARACTER | DataError | 8.4: 22023, 9.7: 22023 | invalid option end character |
| 3563 | ER_INVALID_OPTION_CHARACTERS | DataError | 8.4: 22023, 9.7: 22023 | invalid option characters |
| 3564 | ER_DUPLICATE_OPTION_KEY | DataError | 8.4: 22023, 9.7: 22023 | duplicate option key |
| 3565 | ER_WARN_SRS_NOT_FOUND_AXIS_ORDER | DatabaseError | 8.4: 01000, 9.7: 01000 | warn spatial reference system not found axis order |
| 3566 | ER_NO_ACCESS_TO_NATIVE_FCT | DatabaseError | 8.4: HY000, 9.7: HY000 | no access to native fct |
| 3567 | ER_RESET_SOURCE_TO_VALUE_OUT_OF_RANGE | DatabaseError | 8.4: HY000, 9.7: HY000 | reset source to value out of range |
| 3568 | ER_UNRESOLVED_TABLE_LOCK | DatabaseError | 8.4: HY000, 9.7: HY000 | unresolved table lock |
| 3569 | ER_DUPLICATE_TABLE_LOCK | DatabaseError | 8.4: HY000, 9.7: HY000 | duplicate table lock |
| 3570 | ER_BINLOG_UNSAFE_SKIP_LOCKED | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe skip locked |
| 3571 | ER_BINLOG_UNSAFE_NOWAIT | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe nowait |
| 3572 | ER_LOCK_NOWAIT | DatabaseError | 8.4: HY000, 9.7: HY000 | lock nowait |
| 3573 | ER_CTE_RECURSIVE_REQUIRES_UNION | DatabaseError | 8.4: HY000, 9.7: HY000 | cte recursive requires union |
| 3574 | ER_CTE_RECURSIVE_REQUIRES_NONRECURSIVE_FIRST | DatabaseError | 8.4: HY000, 9.7: HY000 | cte recursive requires nonrecursive first |
| 3575 | ER_CTE_RECURSIVE_FORBIDS_AGGREGATION | DatabaseError | 8.4: HY000, 9.7: HY000 | cte recursive forbids aggregation |
| 3576 | ER_CTE_RECURSIVE_FORBIDDEN_JOIN_ORDER | DatabaseError | 8.4: HY000, 9.7: HY000 | cte recursive forbidden join order |
| 3577 | ER_CTE_RECURSIVE_REQUIRES_SINGLE_REFERENCE | DatabaseError | 8.4: HY000, 9.7: HY000 | cte recursive requires single reference |
| 3578 | ER_SWITCH_TMP_ENGINE | DatabaseError | 8.4: HY000, 9.7: HY000 | switch tmp engine |
| 3579 | ER_WINDOW_NO_SUCH_WINDOW | DatabaseError | 8.4: HY000, 9.7: HY000 | window no such window |
| 3580 | ER_WINDOW_CIRCULARITY_IN_WINDOW_GRAPH | DatabaseError | 8.4: HY000, 9.7: HY000 | window circularity in window graph |
| 3581 | ER_WINDOW_NO_CHILD_PARTITIONING | DatabaseError | 8.4: HY000, 9.7: HY000 | window no child partitioning |
| 3582 | ER_WINDOW_NO_INHERIT_FRAME | DatabaseError | 8.4: HY000, 9.7: HY000 | window no inherit frame |
| 3583 | ER_WINDOW_NO_REDEFINE_ORDER_BY | DatabaseError | 8.4: HY000, 9.7: HY000 | window no redefine order by |
| 3584 | ER_WINDOW_FRAME_START_ILLEGAL | DatabaseError | 8.4: HY000, 9.7: HY000 | window frame start illegal |
| 3585 | ER_WINDOW_FRAME_END_ILLEGAL | DatabaseError | 8.4: HY000, 9.7: HY000 | window frame end illegal |
| 3586 | ER_WINDOW_FRAME_ILLEGAL | DatabaseError | 8.4: HY000, 9.7: HY000 | window frame illegal |
| 3587 | ER_WINDOW_RANGE_FRAME_ORDER_TYPE | DatabaseError | 8.4: HY000, 9.7: HY000 | window range frame order type |
| 3588 | ER_WINDOW_RANGE_FRAME_TEMPORAL_TYPE | DatabaseError | 8.4: HY000, 9.7: HY000 | window range frame temporal type |
| 3589 | ER_WINDOW_RANGE_FRAME_NUMERIC_TYPE | DatabaseError | 8.4: HY000, 9.7: HY000 | window range frame numeric type |
| 3590 | ER_WINDOW_RANGE_BOUND_NOT_CONSTANT | DatabaseError | 8.4: HY000, 9.7: HY000 | window range bound not constant |
| 3591 | ER_WINDOW_DUPLICATE_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | window duplicate name |
| 3592 | ER_WINDOW_ILLEGAL_ORDER_BY | DatabaseError | 8.4: HY000, 9.7: HY000 | window illegal order by |
| 3593 | ER_WINDOW_INVALID_WINDOW_FUNC_USE | DatabaseError | 8.4: HY000, 9.7: HY000 | window invalid window function use |
| 3594 | ER_WINDOW_INVALID_WINDOW_FUNC_ALIAS_USE | DatabaseError | 8.4: HY000, 9.7: HY000 | window invalid window function alias use |
| 3595 | ER_WINDOW_NESTED_WINDOW_FUNC_USE_IN_WINDOW_SPEC | DatabaseError | 8.4: HY000, 9.7: HY000 | window nested window function use in window spec |
| 3596 | ER_WINDOW_ROWS_INTERVAL_USE | DatabaseError | 8.4: HY000, 9.7: HY000 | window rows interval use |
| 3597 | ER_WINDOW_NO_GROUP_ORDER_UNUSED | DatabaseError | 8.4: HY000, 9.7: HY000 | window no group order unused |
| 3598 | ER_WINDOW_EXPLAIN_JSON | DatabaseError | 8.4: HY000, 9.7: HY000 | window explain JSON |
| 3599 | ER_WINDOW_FUNCTION_IGNORES_FRAME | DatabaseError | 8.4: HY000, 9.7: HY000 | window function ignores frame |
| 3600 | ER_WL9236_NOW_UNUSED | DatabaseError | 8.4: HY000, 9.7: HY000 | wl9236 now unused |
| 3601 | ER_INVALID_NO_OF_ARGS | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid no of arguments |
| 3602 | ER_FIELD_IN_GROUPING_NOT_GROUP_BY | DatabaseError | 8.4: HY000, 9.7: HY000 | field in grouping not group by |
| 3603 | ER_TOO_LONG_TABLESPACE_COMMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | too long tablespace comment |
| 3604 | ER_ENGINE_CANT_DROP_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | engine cannot drop table |
| 3605 | ER_ENGINE_CANT_DROP_MISSING_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | engine cannot drop missing table |
| 3606 | ER_TABLESPACE_DUP_FILENAME | DatabaseError | 8.4: HY000, 9.7: HY000 | tablespace dup filename |
| 3607 | ER_DB_DROP_RMDIR2 | DatabaseError | 8.4: HY000, 9.7: HY000 | database drop rmdir2 |
| 3608 | ER_IMP_NO_FILES_MATCHED | DatabaseError | 8.4: HY000, 9.7: HY000 | imp no files matched |
| 3609 | ER_IMP_SCHEMA_DOES_NOT_EXIST | DatabaseError | 8.4: HY000, 9.7: HY000 | imp schema does not exist |
| 3610 | ER_IMP_TABLE_ALREADY_EXISTS | DatabaseError | 8.4: HY000, 9.7: HY000 | imp table already exists |
| 3611 | ER_IMP_INCOMPATIBLE_MYSQLD_VERSION | DatabaseError | 8.4: HY000, 9.7: HY000 | imp incompatible mysqld version |
| 3612 | ER_IMP_INCOMPATIBLE_DD_VERSION | DatabaseError | 8.4: HY000, 9.7: HY000 | imp incompatible dd version |
| 3613 | ER_IMP_INCOMPATIBLE_SDI_VERSION | DatabaseError | 8.4: HY000, 9.7: HY000 | imp incompatible sdi version |
| 3614 | ER_WARN_INVALID_HINT | DatabaseError | 8.4: HY000, 9.7: HY000 | warn invalid hint |
| 3615 | ER_VAR_DOES_NOT_EXIST | DatabaseError | 8.4: HY000, 9.7: HY000 | variable does not exist |
| 3616 | ER_LONGITUDE_OUT_OF_RANGE | DataError | 8.4: 22S02, 9.7: 22S02 | longitude out of range |
| 3617 | ER_LATITUDE_OUT_OF_RANGE | DataError | 8.4: 22S03, 9.7: 22S03 | latitude out of range |
| 3618 | ER_NOT_IMPLEMENTED_FOR_GEOGRAPHIC_SRS | DataError | 8.4: 22S00, 9.7: 22S00 | not implemented for geographic spatial reference system |
| 3619 | ER_ILLEGAL_PRIVILEGE_LEVEL | DatabaseError | 8.4: HY000, 9.7: HY000 | illegal privilege level |
| 3620 | ER_NO_SYSTEM_VIEW_ACCESS | DatabaseError | 8.4: HY000, 9.7: HY000 | no system view access |
| 3621 | ER_COMPONENT_FILTER_FLABBERGASTED | DatabaseError | 8.4: HY000, 9.7: HY000 | component filter flabbergasted |
| 3622 | ER_PART_EXPR_TOO_LONG | DatabaseError | 8.4: HY000, 9.7: HY000 | part expr too long |
| 3623 | ER_UDF_DROP_DYNAMICALLY_REGISTERED | DatabaseError | 8.4: HY000, 9.7: HY000 | UDF drop dynamically registered |
| 3624 | ER_UNABLE_TO_STORE_COLUMN_STATISTICS | DatabaseError | 8.4: HY000, 9.7: HY000 | unable to store column statistics |
| 3625 | ER_UNABLE_TO_UPDATE_COLUMN_STATISTICS | DatabaseError | 8.4: HY000, 9.7: HY000 | unable to update column statistics |
| 3626 | ER_UNABLE_TO_DROP_COLUMN_STATISTICS | DatabaseError | 8.4: HY000, 9.7: HY000 | unable to drop column statistics |
| 3627 | ER_UNABLE_TO_BUILD_HISTOGRAM | DatabaseError | 8.4: HY000, 9.7: HY000 | unable to build histogram |
| 3628 | ER_MANDATORY_ROLE | DatabaseError | 8.4: HY000, 9.7: HY000 | mandatory role |
| 3629 | ER_MISSING_TABLESPACE_FILE | DatabaseError | 8.4: HY000, 9.7: HY000 | missing tablespace file |
| 3630 | ER_PERSIST_ONLY_ACCESS_DENIED_ERROR | ProgrammingError | 8.4: 42000, 9.7: 42000 | persist only access denied error |
| 3631 | ER_CMD_NEED_SUPER | DatabaseError | 8.4: HY000 | cmd need super |
| 3632 | ER_PATH_IN_DATADIR | DatabaseError | 8.4: HY000, 9.7: HY000 | path in datadir |
| 3633 | ER_CLONE_DDL_IN_PROGRESS | DatabaseError | 8.4: HY000, 9.7: HY000 | clone DDL in progress |
| 3634 | ER_CLONE_TOO_MANY_CONCURRENT_CLONES | DatabaseError | 8.4: HY000, 9.7: HY000 | clone too many concurrent clones |
| 3635 | ER_APPLIER_LOG_EVENT_VALIDATION_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | applier log event validation error |
| 3636 | ER_CTE_MAX_RECURSION_DEPTH | DatabaseError | 8.4: HY000, 9.7: HY000 | cte max recursion depth |
| 3637 | ER_NOT_HINT_UPDATABLE_VARIABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | not hint updatable variable |
| 3638 | ER_CREDENTIALS_CONTRADICT_TO_HISTORY | DatabaseError | 8.4: HY000, 9.7: HY000 | credentials contradict to history |
| 3639 | ER_WARNING_PASSWORD_HISTORY_CLAUSES_VOID | DatabaseError | 8.4: HY000, 9.7: HY000 | warning password history clauses void |
| 3640 | ER_CLIENT_DOES_NOT_SUPPORT | DatabaseError | 8.4: HY000, 9.7: HY000 | client does not support |
| 3641 | ER_I_S_SKIPPED_TABLESPACE | DatabaseError | 8.4: HY000, 9.7: HY000 | i s skipped tablespace |
| 3642 | ER_TABLESPACE_ENGINE_MISMATCH | DatabaseError | 8.4: HY000, 9.7: HY000 | tablespace engine mismatch |
| 3643 | ER_WRONG_SRID_FOR_COLUMN | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid srid for column |
| 3644 | ER_CANNOT_ALTER_SRID_DUE_TO_INDEX | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot alter srid due to index |
| 3645 | ER_WARN_BINLOG_PARTIAL_UPDATES_DISABLED | DatabaseError | 8.4: HY000, 9.7: HY000 | warn binlog partial updates disabled |
| 3647 | ER_WARN_BINLOG_PARTIAL_UPDATES_SUGGESTS_PARTIAL_IMAGES | DatabaseError | 8.4: HY000, 9.7: HY000 | warn binlog partial updates suggests partial images |
| 3648 | ER_COULD_NOT_APPLY_JSON_DIFF | DatabaseError | 8.4: HY000, 9.7: HY000 | could not apply JSON diff |
| 3649 | ER_CORRUPTED_JSON_DIFF | DatabaseError | 8.4: HY000, 9.7: HY000 | corrupted JSON diff |
| 3650 | ER_RESOURCE_GROUP_EXISTS | DatabaseError | 8.4: HY000, 9.7: HY000 | resource group exists |
| 3651 | ER_RESOURCE_GROUP_NOT_EXISTS | DatabaseError | 8.4: HY000, 9.7: HY000 | resource group not exists |
| 3652 | ER_INVALID_VCPU_ID | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid vcpu id |
| 3653 | ER_INVALID_VCPU_RANGE | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid vcpu range |
| 3654 | ER_INVALID_THREAD_PRIORITY | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid thread priority |
| 3655 | ER_DISALLOWED_OPERATION | DatabaseError | 8.4: HY000, 9.7: HY000 | disallowed operation |
| 3656 | ER_RESOURCE_GROUP_BUSY | DatabaseError | 8.4: HY000, 9.7: HY000 | resource group busy |
| 3657 | ER_RESOURCE_GROUP_DISABLED | DatabaseError | 8.4: HY000, 9.7: HY000 | resource group disabled |
| 3658 | ER_FEATURE_UNSUPPORTED | DatabaseError | 8.4: HY000, 9.7: HY000 | feature unsupported |
| 3659 | ER_ATTRIBUTE_IGNORED | DatabaseError | 8.4: HY000, 9.7: HY000 | attribute ignored |
| 3660 | ER_INVALID_THREAD_ID | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid thread id |
| 3661 | ER_RESOURCE_GROUP_BIND_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | resource group bind failed |
| 3662 | ER_INVALID_USE_OF_FORCE_OPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid use of force option |
| 3663 | ER_GROUP_REPLICATION_COMMAND_FAILURE | DatabaseError | 8.4: HY000, 9.7: HY000 | group replication command failure |
| 3664 | ER_SDI_OPERATION_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | sdi operation failed |
| 3665 | ER_MISSING_JSON_TABLE_VALUE | DataError | 8.4: 22035, 9.7: 22035 | missing JSON table value |
| 3666 | ER_WRONG_JSON_TABLE_VALUE | DataError | 8.4: 2203F, 9.7: 2203F | invalid JSON table value |
| 3667 | ER_TF_MUST_HAVE_ALIAS | ProgrammingError | 8.4: 42000, 9.7: 42000 | tf must have alias |
| 3668 | ER_TF_FORBIDDEN_JOIN_TYPE | DatabaseError | 8.4: HY000, 9.7: HY000 | tf forbidden join type |
| 3669 | ER_JT_VALUE_OUT_OF_RANGE | DataError | 8.4: 22003, 9.7: 22003 | jt value out of range |
| 3670 | ER_JT_MAX_NESTED_PATH | ProgrammingError | 8.4: 42000, 9.7: 42000 | jt max nested path |
| 3671 | ER_PASSWORD_EXPIRATION_NOT_SUPPORTED_BY_AUTH_METHOD | DatabaseError | 8.4: HY000, 9.7: HY000 | password expiration not supported by auth method |
| 3672 | ER_INVALID_GEOJSON_CRS_NOT_TOP_LEVEL | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid geojson crs not top level |
| 3673 | ER_BAD_NULL_ERROR_NOT_IGNORED | IntegrityError | 8.4: 23000, 9.7: 23000 | bad null error not ignored |
| 3674 | WARN_USELESS_SPATIAL_INDEX | DatabaseError | 8.4: HY000, 9.7: HY000 | useless spatial index |
| 3675 | ER_DISK_FULL_NOWAIT | DatabaseError | 8.4: HY000, 9.7: HY000 | disk full nowait |
| 3676 | ER_PARSE_ERROR_IN_DIGEST_FN | DatabaseError | 8.4: HY000, 9.7: HY000 | parse error in digest fn |
| 3677 | ER_UNDISCLOSED_PARSE_ERROR_IN_DIGEST_FN | DatabaseError | 8.4: HY000, 9.7: HY000 | undisclosed parse error in digest fn |
| 3678 | ER_SCHEMA_DIR_EXISTS | DatabaseError | 8.4: HY000, 9.7: HY000 | schema dir exists |
| 3679 | ER_SCHEMA_DIR_MISSING | DatabaseError | 8.4: HY000, 9.7: HY000 | schema dir missing |
| 3680 | ER_SCHEMA_DIR_CREATE_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | schema dir create failed |
| 3681 | ER_SCHEMA_DIR_UNKNOWN | DatabaseError | 8.4: HY000, 9.7: HY000 | schema dir unknown |
| 3682 | ER_ONLY_IMPLEMENTED_FOR_SRID_0_AND_4326 | DataError | 8.4: 22S00, 9.7: 22S00 | only implemented for srid 0 and 4326 |
| 3684 | ER_REGEXP_BUFFER_OVERFLOW | DatabaseError | 8.4: HY000, 9.7: HY000 | regexp buffer overflow |
| 3685 | ER_REGEXP_ILLEGAL_ARGUMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | regexp illegal argument |
| 3686 | ER_REGEXP_INDEX_OUTOFBOUNDS_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | regexp index outofbounds error |
| 3687 | ER_REGEXP_INTERNAL_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | regexp internal error |
| 3688 | ER_REGEXP_RULE_SYNTAX | DatabaseError | 8.4: HY000, 9.7: HY000 | regexp rule syntax |
| 3689 | ER_REGEXP_BAD_ESCAPE_SEQUENCE | DatabaseError | 8.4: HY000, 9.7: HY000 | regexp bad escape sequence |
| 3690 | ER_REGEXP_UNIMPLEMENTED | DatabaseError | 8.4: HY000, 9.7: HY000 | regexp unimplemented |
| 3691 | ER_REGEXP_MISMATCHED_PAREN | DatabaseError | 8.4: HY000, 9.7: HY000 | regexp mismatched paren |
| 3692 | ER_REGEXP_BAD_INTERVAL | DatabaseError | 8.4: HY000, 9.7: HY000 | regexp bad interval |
| 3693 | ER_REGEXP_MAX_LT_MIN | DatabaseError | 8.4: HY000, 9.7: HY000 | regexp max lt min |
| 3694 | ER_REGEXP_INVALID_BACK_REF | DatabaseError | 8.4: HY000, 9.7: HY000 | regexp invalid back reference |
| 3695 | ER_REGEXP_LOOK_BEHIND_LIMIT | DatabaseError | 8.4: HY000, 9.7: HY000 | regexp look behind limit |
| 3696 | ER_REGEXP_MISSING_CLOSE_BRACKET | DatabaseError | 8.4: HY000, 9.7: HY000 | regexp missing close bracket |
| 3697 | ER_REGEXP_INVALID_RANGE | DatabaseError | 8.4: HY000, 9.7: HY000 | regexp invalid range |
| 3698 | ER_REGEXP_STACK_OVERFLOW | DatabaseError | 8.4: HY000, 9.7: HY000 | regexp stack overflow |
| 3699 | ER_REGEXP_TIME_OUT | DatabaseError | 8.4: HY000, 9.7: HY000 | regexp time out |
| 3700 | ER_REGEXP_PATTERN_TOO_BIG | DatabaseError | 8.4: HY000, 9.7: HY000 | regexp pattern too big |
| 3701 | ER_CANT_SET_ERROR_LOG_SERVICE | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot set error log service |
| 3702 | ER_EMPTY_PIPELINE_FOR_ERROR_LOG_SERVICE | DatabaseError | 8.4: HY000, 9.7: HY000 | empty pipeline for error log service |
| 3703 | ER_COMPONENT_FILTER_DIAGNOSTICS | DatabaseError | 8.4: HY000, 9.7: HY000 | component filter diagnostics |
| 3704 | ER_NOT_IMPLEMENTED_FOR_CARTESIAN_SRS | DataError | 8.4: 22S00, 9.7: 22S00 | not implemented for cartesian spatial reference system |
| 3705 | ER_NOT_IMPLEMENTED_FOR_PROJECTED_SRS | DataError | 8.4: 22S00, 9.7: 22S00 | not implemented for projected spatial reference system |
| 3706 | ER_NONPOSITIVE_RADIUS | DataError | 8.4: 22003, 9.7: 22003 | nonpositive radius |
| 3707 | ER_RESTART_SERVER_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | restart server failed |
| 3708 | ER_SRS_MISSING_MANDATORY_ATTRIBUTE | DatabaseError | 8.4: SR006, 9.7: SR006 | spatial reference system missing mandatory attribute |
| 3709 | ER_SRS_MULTIPLE_ATTRIBUTE_DEFINITIONS | DatabaseError | 8.4: SR006, 9.7: SR006 | spatial reference system multiple attribute definitions |
| 3710 | ER_SRS_NAME_CANT_BE_EMPTY_OR_WHITESPACE | DatabaseError | 8.4: SR006, 9.7: SR006 | spatial reference system name cannot be empty or whitespace |
| 3711 | ER_SRS_ORGANIZATION_CANT_BE_EMPTY_OR_WHITESPACE | DatabaseError | 8.4: SR006, 9.7: SR006 | spatial reference system organization cannot be empty or whitespace |
| 3712 | ER_SRS_ID_ALREADY_EXISTS | DatabaseError | 8.4: SR004, 9.7: SR004 | spatial reference system id already exists |
| 3713 | ER_WARN_SRS_ID_ALREADY_EXISTS | DatabaseError | 8.4: 01S00, 9.7: 01S00 | warn spatial reference system id already exists |
| 3714 | ER_CANT_MODIFY_SRID_0 | DatabaseError | 8.4: SR000, 9.7: SR000 | cannot modify srid 0 |
| 3715 | ER_WARN_RESERVED_SRID_RANGE | DatabaseError | 8.4: 01S01, 9.7: 01S01 | warn reserved srid range |
| 3716 | ER_CANT_MODIFY_SRS_USED_BY_COLUMN | DatabaseError | 8.4: SR005, 9.7: SR005 | cannot modify spatial reference system used by column |
| 3717 | ER_SRS_INVALID_CHARACTER_IN_ATTRIBUTE | DatabaseError | 8.4: SR006, 9.7: SR006 | spatial reference system invalid character in attribute |
| 3718 | ER_SRS_ATTRIBUTE_STRING_TOO_LONG | DatabaseError | 8.4: SR006, 9.7: SR006 | spatial reference system attribute string too long |
| 3719 | ER_DEPRECATED_UTF8_ALIAS | DatabaseError | 8.4: HY000, 9.7: HY000 | deprecated utf8 alias |
| 3720 | ER_DEPRECATED_NATIONAL | DatabaseError | 8.4: HY000, 9.7: HY000 | deprecated national |
| 3721 | ER_INVALID_DEFAULT_UTF8MB4_COLLATION | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid default utf8mb4 collation |
| 3722 | ER_UNABLE_TO_COLLECT_LOG_STATUS | DatabaseError | 8.4: HY000, 9.7: HY000 | unable to collect log status |
| 3723 | ER_RESERVED_TABLESPACE_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | reserved tablespace name |
| 3724 | ER_UNABLE_TO_SET_OPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | unable to set option |
| 3725 | ER_REPLICA_POSSIBLY_DIVERGED_AFTER_DDL | DatabaseError | 8.4: HY000, 9.7: HY000 | replica possibly diverged after DDL |
| 3726 | ER_SRS_NOT_GEOGRAPHIC | DataError | 8.4: 22S00, 9.7: 22S00 | spatial reference system not geographic |
| 3727 | ER_POLYGON_TOO_LARGE | DataError | 8.4: 22023, 9.7: 22023 | polygon too large |
| 3728 | ER_SPATIAL_UNIQUE_INDEX | DatabaseError | 8.4: HY000, 9.7: HY000 | spatial unique index |
| 3729 | ER_INDEX_TYPE_NOT_SUPPORTED_FOR_SPATIAL_INDEX | DatabaseError | 8.4: HY000, 9.7: HY000 | index type not supported for spatial index |
| 3730 | ER_FK_CANNOT_DROP_PARENT | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign key cannot drop parent |
| 3731 | ER_GEOMETRY_PARAM_LONGITUDE_OUT_OF_RANGE | DataError | 8.4: 22S02, 9.7: 22S02 | geometry param longitude out of range |
| 3732 | ER_GEOMETRY_PARAM_LATITUDE_OUT_OF_RANGE | DataError | 8.4: 22S03, 9.7: 22S03 | geometry param latitude out of range |
| 3733 | ER_FK_CANNOT_USE_VIRTUAL_COLUMN | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign key cannot use virtual column |
| 3734 | ER_FK_NO_COLUMN_PARENT | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign key no column parent |
| 3735 | ER_CANT_SET_ERROR_SUPPRESSION_LIST | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot set error suppression list |
| 3736 | ER_SRS_GEOGCS_INVALID_AXES | DatabaseError | 8.4: SR002, 9.7: SR002 | spatial reference system geogcs invalid axes |
| 3737 | ER_SRS_INVALID_SEMI_MAJOR_AXIS | DatabaseError | 8.4: SR002, 9.7: SR002 | spatial reference system invalid semi major axis |
| 3738 | ER_SRS_INVALID_INVERSE_FLATTENING | DatabaseError | 8.4: SR002, 9.7: SR002 | spatial reference system invalid inverse flattening |
| 3739 | ER_SRS_INVALID_ANGULAR_UNIT | DatabaseError | 8.4: SR002, 9.7: SR002 | spatial reference system invalid angular unit |
| 3740 | ER_SRS_INVALID_PRIME_MERIDIAN | DatabaseError | 8.4: SR002, 9.7: SR002 | spatial reference system invalid prime meridian |
| 3741 | ER_TRANSFORM_SOURCE_SRS_NOT_SUPPORTED | DataError | 8.4: 22S00, 9.7: 22S00 | transform source spatial reference system not supported |
| 3742 | ER_TRANSFORM_TARGET_SRS_NOT_SUPPORTED | DataError | 8.4: 22S00, 9.7: 22S00 | transform target spatial reference system not supported |
| 3743 | ER_TRANSFORM_SOURCE_SRS_MISSING_TOWGS84 | DataError | 8.4: 22S00, 9.7: 22S00 | transform source spatial reference system missing towgs84 |
| 3744 | ER_TRANSFORM_TARGET_SRS_MISSING_TOWGS84 | DataError | 8.4: 22S00, 9.7: 22S00 | transform target spatial reference system missing towgs84 |
| 3745 | ER_TEMP_TABLE_PREVENTS_SWITCH_SESSION_BINLOG_FORMAT | DatabaseError | 8.4: HY000, 9.7: HY000 | temp table prevents switch session binlog format |
| 3746 | ER_TEMP_TABLE_PREVENTS_SWITCH_GLOBAL_BINLOG_FORMAT | DatabaseError | 8.4: HY000, 9.7: HY000 | temp table prevents switch global binlog format |
| 3747 | ER_RUNNING_APPLIER_PREVENTS_SWITCH_GLOBAL_BINLOG_FORMAT | DatabaseError | 8.4: HY000, 9.7: HY000 | running applier prevents switch global binlog format |
| 3748 | ER_CLIENT_GTID_UNSAFE_CREATE_DROP_TEMP_TABLE_IN_TRX_IN_SBR | DatabaseError | 8.4: HY000, 9.7: HY000 | client GTID unsafe create drop temp table in transaction in sbr |
| 3750 | ER_TABLE_WITHOUT_PK | DatabaseError | 8.4: HY000, 9.7: HY000 | table without primary key |
| 3751 | ER_WARN_DATA_TRUNCATED_FUNCTIONAL_INDEX | DatabaseError | 8.4: 01000, 9.7: 01000 | warn data truncated functional index |
| 3752 | ER_WARN_DATA_OUT_OF_RANGE_FUNCTIONAL_INDEX | DataError | 8.4: 22003, 9.7: 22003 | warn data out of range functional index |
| 3753 | ER_FUNCTIONAL_INDEX_ON_JSON_OR_GEOMETRY_FUNCTION | ProgrammingError | 8.4: 42000, 9.7: 42000 | functional index on JSON or geometry function |
| 3754 | ER_FUNCTIONAL_INDEX_REF_AUTO_INCREMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | functional index reference auto increment |
| 3755 | ER_CANNOT_DROP_COLUMN_FUNCTIONAL_INDEX | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot drop column functional index |
| 3756 | ER_FUNCTIONAL_INDEX_PRIMARY_KEY | DatabaseError | 8.4: HY000, 9.7: HY000 | functional index primary key |
| 3757 | ER_FUNCTIONAL_INDEX_ON_LOB | DatabaseError | 8.4: HY000, 9.7: HY000 | functional index on lob |
| 3758 | ER_FUNCTIONAL_INDEX_FUNCTION_IS_NOT_ALLOWED | DatabaseError | 8.4: HY000, 9.7: HY000 | functional index function is not allowed |
| 3759 | ER_FULLTEXT_FUNCTIONAL_INDEX | DatabaseError | 8.4: HY000, 9.7: HY000 | fulltext functional index |
| 3760 | ER_SPATIAL_FUNCTIONAL_INDEX | DatabaseError | 8.4: HY000, 9.7: HY000 | spatial functional index |
| 3761 | ER_WRONG_KEY_COLUMN_FUNCTIONAL_INDEX | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid key column functional index |
| 3762 | ER_FUNCTIONAL_INDEX_ON_FIELD | DatabaseError | 8.4: HY000, 9.7: HY000 | functional index on field |
| 3763 | ER_GENERATED_COLUMN_NAMED_FUNCTION_IS_NOT_ALLOWED | DatabaseError | 8.4: HY000, 9.7: HY000 | generated column named function is not allowed |
| 3764 | ER_GENERATED_COLUMN_ROW_VALUE | DatabaseError | 8.4: HY000, 9.7: HY000 | generated column row value |
| 3765 | ER_GENERATED_COLUMN_VARIABLES | DatabaseError | 8.4: HY000, 9.7: HY000 | generated column variables |
| 3766 | ER_DEPENDENT_BY_DEFAULT_GENERATED_VALUE | DatabaseError | 8.4: HY000, 9.7: HY000 | dependent by default generated value |
| 3767 | ER_DEFAULT_VAL_GENERATED_NON_PRIOR | DatabaseError | 8.4: HY000, 9.7: HY000 | default val generated non prior |
| 3768 | ER_DEFAULT_VAL_GENERATED_REF_AUTO_INC | DatabaseError | 8.4: HY000, 9.7: HY000 | default val generated reference auto inc |
| 3769 | ER_DEFAULT_VAL_GENERATED_FUNCTION_IS_NOT_ALLOWED | DatabaseError | 8.4: HY000, 9.7: HY000 | default val generated function is not allowed |
| 3770 | ER_DEFAULT_VAL_GENERATED_NAMED_FUNCTION_IS_NOT_ALLOWED | DatabaseError | 8.4: HY000, 9.7: HY000 | default val generated named function is not allowed |
| 3771 | ER_DEFAULT_VAL_GENERATED_ROW_VALUE | DatabaseError | 8.4: HY000, 9.7: HY000 | default val generated row value |
| 3772 | ER_DEFAULT_VAL_GENERATED_VARIABLES | DatabaseError | 8.4: HY000, 9.7: HY000 | default val generated variables |
| 3773 | ER_DEFAULT_AS_VAL_GENERATED | DatabaseError | 8.4: HY000, 9.7: HY000 | default as val generated |
| 3774 | ER_UNSUPPORTED_ACTION_ON_DEFAULT_VAL_GENERATED | DatabaseError | 8.4: HY000, 9.7: HY000 | unsupported action on default val generated |
| 3775 | ER_GTID_UNSAFE_ALTER_ADD_COL_WITH_DEFAULT_EXPRESSION | DatabaseError | 8.4: HY000, 9.7: HY000 | GTID unsafe alter add column with default expression |
| 3776 | ER_FK_CANNOT_CHANGE_ENGINE | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign key cannot change engine |
| 3777 | ER_WARN_DEPRECATED_USER_SET_EXPR | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated user set expr |
| 3778 | ER_WARN_DEPRECATED_UTF8MB3_COLLATION | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated utf8mb3 collation |
| 3779 | ER_WARN_DEPRECATED_NESTED_COMMENT_SYNTAX | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated nested comment syntax |
| 3780 | ER_FK_INCOMPATIBLE_COLUMNS | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign key incompatible columns |
| 3781 | ER_GR_HOLD_WAIT_TIMEOUT | DatabaseError | 8.4: HY000, 9.7: HY000 | gr hold wait timeout |
| 3782 | ER_GR_HOLD_KILLED | DatabaseError | 8.4: HY000, 9.7: HY000 | gr hold killed |
| 3783 | ER_GR_HOLD_MEMBER_STATUS_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | gr hold member status error |
| 3784 | ER_RPL_ENCRYPTION_FAILED_TO_FETCH_KEY | DatabaseError | 8.4: HY000, 9.7: HY000 | rpl encryption failed to fetch key |
| 3785 | ER_RPL_ENCRYPTION_KEY_NOT_FOUND | DatabaseError | 8.4: HY000, 9.7: HY000 | rpl encryption key not found |
| 3786 | ER_RPL_ENCRYPTION_KEYRING_INVALID_KEY | DatabaseError | 8.4: HY000, 9.7: HY000 | rpl encryption keyring invalid key |
| 3787 | ER_RPL_ENCRYPTION_HEADER_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | rpl encryption header error |
| 3788 | ER_RPL_ENCRYPTION_FAILED_TO_ROTATE_LOGS | DatabaseError | 8.4: HY000, 9.7: HY000 | rpl encryption failed to rotate logs |
| 3789 | ER_RPL_ENCRYPTION_KEY_EXISTS_UNEXPECTED | DatabaseError | 8.4: HY000, 9.7: HY000 | rpl encryption key exists unexpected |
| 3790 | ER_RPL_ENCRYPTION_FAILED_TO_GENERATE_KEY | DatabaseError | 8.4: HY000, 9.7: HY000 | rpl encryption failed to generate key |
| 3791 | ER_RPL_ENCRYPTION_FAILED_TO_STORE_KEY | DatabaseError | 8.4: HY000, 9.7: HY000 | rpl encryption failed to store key |
| 3792 | ER_RPL_ENCRYPTION_FAILED_TO_REMOVE_KEY | DatabaseError | 8.4: HY000, 9.7: HY000 | rpl encryption failed to remove key |
| 3793 | ER_RPL_ENCRYPTION_UNABLE_TO_CHANGE_OPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | rpl encryption unable to change option |
| 3794 | ER_RPL_ENCRYPTION_MASTER_KEY_RECOVERY_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | rpl encryption master key recovery failed |
| 3795 | ER_SLOW_LOG_MODE_IGNORED_WHEN_NOT_LOGGING_TO_FILE | DatabaseError | 8.4: HY000, 9.7: HY000 | slow log mode ignored when not logging to file |
| 3796 | ER_GRP_TRX_CONSISTENCY_NOT_ALLOWED | DatabaseError | 8.4: HY000, 9.7: HY000 | grp transaction consistency not allowed |
| 3797 | ER_GRP_TRX_CONSISTENCY_BEFORE | DatabaseError | 8.4: HY000, 9.7: HY000 | grp transaction consistency before |
| 3798 | ER_GRP_TRX_CONSISTENCY_AFTER_ON_TRX_BEGIN | DatabaseError | 8.4: HY000, 9.7: HY000 | grp transaction consistency after on transaction begin |
| 3799 | ER_GRP_TRX_CONSISTENCY_BEGIN_NOT_ALLOWED | DatabaseError | 8.4: HY000, 9.7: HY000 | grp transaction consistency begin not allowed |
| 3800 | ER_FUNCTIONAL_INDEX_ROW_VALUE_IS_NOT_ALLOWED | DatabaseError | 8.4: HY000, 9.7: HY000 | functional index row value is not allowed |
| 3801 | ER_RPL_ENCRYPTION_FAILED_TO_ENCRYPT | DatabaseError | 8.4: HY000, 9.7: HY000 | rpl encryption failed to encrypt |
| 3802 | ER_PAGE_TRACKING_NOT_STARTED | DatabaseError | 8.4: HY000, 9.7: HY000 | page tracking not started |
| 3803 | ER_PAGE_TRACKING_RANGE_NOT_TRACKED | DatabaseError | 8.4: HY000, 9.7: HY000 | page tracking range not tracked |
| 3804 | ER_PAGE_TRACKING_CANNOT_PURGE | DatabaseError | 8.4: HY000, 9.7: HY000 | page tracking cannot purge |
| 3805 | ER_RPL_ENCRYPTION_CANNOT_ROTATE_BINLOG_MASTER_KEY | DatabaseError | 8.4: HY000, 9.7: HY000 | rpl encryption cannot rotate binlog master key |
| 3806 | ER_BINLOG_MASTER_KEY_RECOVERY_OUT_OF_COMBINATION | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog master key recovery out of combination |
| 3807 | ER_BINLOG_MASTER_KEY_ROTATION_FAIL_TO_OPERATE_KEY | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog master key rotation fail to operate key |
| 3808 | ER_BINLOG_MASTER_KEY_ROTATION_FAIL_TO_ROTATE_LOGS | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog master key rotation fail to rotate logs |
| 3809 | ER_BINLOG_MASTER_KEY_ROTATION_FAIL_TO_REENCRYPT_LOG | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog master key rotation fail to reencrypt log |
| 3810 | ER_BINLOG_MASTER_KEY_ROTATION_FAIL_TO_CLEANUP_UNUSED_KEYS | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog master key rotation fail to cleanup unused keys |
| 3811 | ER_BINLOG_MASTER_KEY_ROTATION_FAIL_TO_CLEANUP_AUX_KEY | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog master key rotation fail to cleanup aux key |
| 3812 | ER_NON_BOOLEAN_EXPR_FOR_CHECK_CONSTRAINT | DatabaseError | 8.4: HY000, 9.7: HY000 | non boolean expr for check constraint |
| 3813 | ER_COLUMN_CHECK_CONSTRAINT_REFERENCES_OTHER_COLUMN | DatabaseError | 8.4: HY000, 9.7: HY000 | column check constraint references other column |
| 3814 | ER_CHECK_CONSTRAINT_NAMED_FUNCTION_IS_NOT_ALLOWED | DatabaseError | 8.4: HY000, 9.7: HY000 | check constraint named function is not allowed |
| 3815 | ER_CHECK_CONSTRAINT_FUNCTION_IS_NOT_ALLOWED | DatabaseError | 8.4: HY000, 9.7: HY000 | check constraint function is not allowed |
| 3816 | ER_CHECK_CONSTRAINT_VARIABLES | DatabaseError | 8.4: HY000, 9.7: HY000 | check constraint variables |
| 3817 | ER_CHECK_CONSTRAINT_ROW_VALUE | DatabaseError | 8.4: HY000, 9.7: HY000 | check constraint row value |
| 3818 | ER_CHECK_CONSTRAINT_REFERS_AUTO_INCREMENT_COLUMN | DatabaseError | 8.4: HY000, 9.7: HY000 | check constraint refers auto increment column |
| 3819 | ER_CHECK_CONSTRAINT_VIOLATED | IntegrityError | 8.4: HY000, 9.7: HY000 | check constraint violated |
| 3820 | ER_CHECK_CONSTRAINT_REFERS_UNKNOWN_COLUMN | DatabaseError | 8.4: HY000, 9.7: HY000 | check constraint refers unknown column |
| 3821 | ER_CHECK_CONSTRAINT_NOT_FOUND | DatabaseError | 8.4: HY000, 9.7: HY000 | check constraint not found |
| 3822 | ER_CHECK_CONSTRAINT_DUP_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | check constraint dup name |
| 3823 | ER_CHECK_CONSTRAINT_CLAUSE_USING_FK_REFER_ACTION_COLUMN | DatabaseError | 8.4: HY000, 9.7: HY000 | check constraint clause using foreign key refer action column |
| 3824 | WARN_UNENCRYPTED_TABLE_IN_ENCRYPTED_DB | DatabaseError | 8.4: HY000, 9.7: HY000 | unencrypted table in encrypted database |
| 3825 | ER_INVALID_ENCRYPTION_REQUEST | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid encryption request |
| 3826 | ER_CANNOT_SET_TABLE_ENCRYPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot set table encryption |
| 3827 | ER_CANNOT_SET_DATABASE_ENCRYPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot set database encryption |
| 3828 | ER_CANNOT_SET_TABLESPACE_ENCRYPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot set tablespace encryption |
| 3829 | ER_TABLESPACE_CANNOT_BE_ENCRYPTED | DatabaseError | 8.4: HY000, 9.7: HY000 | tablespace cannot be encrypted |
| 3830 | ER_TABLESPACE_CANNOT_BE_DECRYPTED | DatabaseError | 8.4: HY000, 9.7: HY000 | tablespace cannot be decrypted |
| 3831 | ER_TABLESPACE_TYPE_UNKNOWN | DatabaseError | 8.4: HY000, 9.7: HY000 | tablespace type unknown |
| 3832 | ER_TARGET_TABLESPACE_UNENCRYPTED | DatabaseError | 8.4: HY000, 9.7: HY000 | target tablespace unencrypted |
| 3833 | ER_CANNOT_USE_ENCRYPTION_CLAUSE | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot use encryption clause |
| 3834 | ER_INVALID_MULTIPLE_CLAUSES | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid multiple clauses |
| 3835 | ER_UNSUPPORTED_USE_OF_GRANT_AS | DatabaseError | 8.4: HY000, 9.7: HY000 | unsupported use of grant as |
| 3836 | ER_UKNOWN_AUTH_ID_OR_ACCESS_DENIED_FOR_GRANT_AS | DatabaseError | 8.4: HY000, 9.7: HY000 | uknown auth id or access denied for grant as |
| 3837 | ER_DEPENDENT_BY_FUNCTIONAL_INDEX | DatabaseError | 8.4: HY000, 9.7: HY000 | dependent by functional index |
| 3838 | ER_PLUGIN_NOT_EARLY | DatabaseError | 8.4: HY000, 9.7: HY000 | plugin not early |
| 3839 | ER_INNODB_REDO_LOG_ARCHIVE_START_SUBDIR_PATH | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb redo log archive start subdir path |
| 3840 | ER_INNODB_REDO_LOG_ARCHIVE_START_TIMEOUT | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb redo log archive start timeout |
| 3841 | ER_INNODB_REDO_LOG_ARCHIVE_DIRS_INVALID | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb redo log archive dirs invalid |
| 3842 | ER_INNODB_REDO_LOG_ARCHIVE_LABEL_NOT_FOUND | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb redo log archive label not found |
| 3843 | ER_INNODB_REDO_LOG_ARCHIVE_DIR_EMPTY | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb redo log archive dir empty |
| 3844 | ER_INNODB_REDO_LOG_ARCHIVE_NO_SUCH_DIR | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb redo log archive no such dir |
| 3845 | ER_INNODB_REDO_LOG_ARCHIVE_DIR_CLASH | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb redo log archive dir clash |
| 3846 | ER_INNODB_REDO_LOG_ARCHIVE_DIR_PERMISSIONS | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb redo log archive dir permissions |
| 3847 | ER_INNODB_REDO_LOG_ARCHIVE_FILE_CREATE | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb redo log archive file create |
| 3848 | ER_INNODB_REDO_LOG_ARCHIVE_ACTIVE | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb redo log archive active |
| 3849 | ER_INNODB_REDO_LOG_ARCHIVE_INACTIVE | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb redo log archive inactive |
| 3850 | ER_INNODB_REDO_LOG_ARCHIVE_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb redo log archive failed |
| 3851 | ER_INNODB_REDO_LOG_ARCHIVE_SESSION | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb redo log archive session |
| 3852 | ER_STD_REGEX_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | std regex error |
| 3853 | ER_INVALID_JSON_TYPE | DataError | 8.4: 22032, 9.7: 22032 | invalid JSON type |
| 3854 | ER_CANNOT_CONVERT_STRING | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot convert string |
| 3855 | ER_DEPENDENT_BY_PARTITION_FUNC | DatabaseError | 8.4: HY000, 9.7: HY000 | dependent by partition function |
| 3856 | ER_WARN_DEPRECATED_FLOAT_AUTO_INCREMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated float auto increment |
| 3857 | ER_RPL_CANT_STOP_REPLICA_WHILE_LOCKED_BACKUP | DatabaseError | 8.4: HY000, 9.7: HY000 | rpl cannot stop replica while locked backup |
| 3858 | ER_WARN_DEPRECATED_FLOAT_DIGITS | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated float digits |
| 3859 | ER_WARN_DEPRECATED_FLOAT_UNSIGNED | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated float unsigned |
| 3860 | ER_WARN_DEPRECATED_INTEGER_DISPLAY_WIDTH | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated integer display width |
| 3861 | ER_WARN_DEPRECATED_ZEROFILL | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated zerofill |
| 3862 | ER_CLONE_DONOR | DatabaseError | 8.4: HY000, 9.7: HY000 | clone donor |
| 3863 | ER_CLONE_PROTOCOL | DatabaseError | 8.4: HY000, 9.7: HY000 | clone protocol |
| 3864 | ER_CLONE_DONOR_VERSION | DatabaseError | 8.4: HY000, 9.7: HY000 | clone donor version |
| 3865 | ER_CLONE_OS | DatabaseError | 8.4: HY000, 9.7: HY000 | clone os |
| 3866 | ER_CLONE_PLATFORM | DatabaseError | 8.4: HY000, 9.7: HY000 | clone platform |
| 3867 | ER_CLONE_CHARSET | DatabaseError | 8.4: HY000, 9.7: HY000 | clone character set |
| 3868 | ER_CLONE_CONFIG | DatabaseError | 8.4: HY000, 9.7: HY000 | clone config |
| 3869 | ER_CLONE_SYS_CONFIG | DatabaseError | 8.4: HY000, 9.7: HY000 | clone sys config |
| 3870 | ER_CLONE_PLUGIN_MATCH | DatabaseError | 8.4: HY000, 9.7: HY000 | clone plugin match |
| 3871 | ER_CLONE_LOOPBACK | DatabaseError | 8.4: HY000, 9.7: HY000 | clone loopback |
| 3872 | ER_CLONE_ENCRYPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | clone encryption |
| 3873 | ER_CLONE_DISK_SPACE | DatabaseError | 8.4: HY000, 9.7: HY000 | clone disk space |
| 3874 | ER_CLONE_IN_PROGRESS | DatabaseError | 8.4: HY000, 9.7: HY000 | clone in progress |
| 3875 | ER_CLONE_DISALLOWED | DatabaseError | 8.4: HY000, 9.7: HY000 | clone disallowed |
| 3876 | ER_CANNOT_GRANT_ROLES_TO_ANONYMOUS_USER | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot grant roles to anonymous user |
| 3877 | ER_SECONDARY_ENGINE_PLUGIN | DatabaseError | 8.4: HY000, 9.7: HY000 | secondary engine plugin |
| 3878 | ER_SECOND_PASSWORD_CANNOT_BE_EMPTY | DatabaseError | 8.4: HY000, 9.7: HY000 | second password cannot be empty |
| 3879 | ER_DB_ACCESS_DENIED | DatabaseError | 8.4: HY000, 9.7: HY000 | database access denied |
| 3880 | ER_DA_AUTH_ID_WITH_SYSTEM_USER_PRIV_IN_MANDATORY_ROLES | DatabaseError | 8.4: HY000, 9.7: HY000 | da auth id with system user priv in mandatory roles |
| 3881 | ER_DA_RPL_GTID_TABLE_CANNOT_OPEN | DatabaseError | 8.4: HY000, 9.7: HY000 | da rpl GTID table cannot open |
| 3882 | ER_GEOMETRY_IN_UNKNOWN_LENGTH_UNIT | DatabaseError | 8.4: SU001, 9.7: SU001 | geometry in unknown length unit |
| 3883 | ER_DA_PLUGIN_INSTALL_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | da plugin install error |
| 3884 | ER_NO_SESSION_TEMP | DatabaseError | 8.4: HY000, 9.7: HY000 | no session temp |
| 3885 | ER_DA_UNKNOWN_ERROR_NUMBER | DatabaseError | 8.4: HY000, 9.7: HY000 | da unknown error number |
| 3886 | ER_COLUMN_CHANGE_SIZE | DatabaseError | 8.4: HY000, 9.7: HY000 | column change size |
| 3887 | ER_REGEXP_INVALID_CAPTURE_GROUP_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | regexp invalid capture group name |
| 3888 | ER_DA_SSL_LIBRARY_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | da SSL library error |
| 3889 | ER_SECONDARY_ENGINE | DatabaseError | 8.4: HY000, 9.7: HY000 | secondary engine |
| 3890 | ER_SECONDARY_ENGINE_DDL | DatabaseError | 8.4: HY000, 9.7: HY000 | secondary engine DDL |
| 3891 | ER_INCORRECT_CURRENT_PASSWORD | DatabaseError | 8.4: HY000, 9.7: HY000 | incorrect current password |
| 3892 | ER_MISSING_CURRENT_PASSWORD | DatabaseError | 8.4: HY000, 9.7: HY000 | missing current password |
| 3893 | ER_CURRENT_PASSWORD_NOT_REQUIRED | DatabaseError | 8.4: HY000, 9.7: HY000 | current password not required |
| 3894 | ER_PASSWORD_CANNOT_BE_RETAINED_ON_PLUGIN_CHANGE | DatabaseError | 8.4: HY000, 9.7: HY000 | password cannot be retained on plugin change |
| 3895 | ER_CURRENT_PASSWORD_CANNOT_BE_RETAINED | DatabaseError | 8.4: HY000, 9.7: HY000 | current password cannot be retained |
| 3896 | ER_PARTIAL_REVOKES_EXIST | DatabaseError | 8.4: HY000, 9.7: HY000 | partial revokes exist |
| 3897 | ER_CANNOT_GRANT_SYSTEM_PRIV_TO_MANDATORY_ROLE | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot grant system priv to mandatory role |
| 3898 | ER_XA_REPLICATION_FILTERS | DatabaseError | 8.4: HY000, 9.7: HY000 | XA replication filters |
| 3899 | ER_UNSUPPORTED_SQL_MODE | DatabaseError | 8.4: HY000, 9.7: HY000 | unsupported SQL mode |
| 3900 | ER_REGEXP_INVALID_FLAG | DatabaseError | 8.4: HY000, 9.7: HY000 | regexp invalid flag |
| 3901 | ER_PARTIAL_REVOKE_AND_DB_GRANT_BOTH_EXISTS | DatabaseError | 8.4: HY000, 9.7: HY000 | partial revoke and database grant both exists |
| 3902 | ER_UNIT_NOT_FOUND | DatabaseError | 8.4: SU001, 9.7: SU001 | unit not found |
| 3903 | ER_INVALID_JSON_VALUE_FOR_FUNC_INDEX | DataError | 8.4: 22018, 9.7: 22018 | invalid JSON value for function index |
| 3904 | ER_JSON_VALUE_OUT_OF_RANGE_FOR_FUNC_INDEX | DataError | 8.4: 22003, 9.7: 22003 | JSON value out of range for function index |
| 3905 | ER_EXCEEDED_MV_KEYS_NUM | DatabaseError | 8.4: HY000, 9.7: HY000 | exceeded mv keys number |
| 3906 | ER_EXCEEDED_MV_KEYS_SPACE | DatabaseError | 8.4: HY000, 9.7: HY000 | exceeded mv keys space |
| 3907 | ER_FUNCTIONAL_INDEX_DATA_IS_TOO_LONG | DataError | 8.4: 22001, 9.7: 22001 | functional index data is too long |
| 3908 | ER_WRONG_MVI_VALUE | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid mvi value |
| 3909 | ER_WARN_FUNC_INDEX_NOT_APPLICABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | warn function index not applicable |
| 3910 | ER_GRP_RPL_UDF_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | grp rpl UDF error |
| 3911 | ER_UPDATE_GTID_PURGED_WITH_GR | DatabaseError | 8.4: HY000, 9.7: HY000 | update GTID purged with gr |
| 3912 | ER_GROUPING_ON_TIMESTAMP_IN_DST | DatabaseError | 8.4: HY000, 9.7: HY000 | grouping on timestamp in dst |
| 3913 | ER_TABLE_NAME_CAUSES_TOO_LONG_PATH | DatabaseError | 8.4: HY000, 9.7: HY000 | table name causes too long path |
| 3914 | ER_AUDIT_LOG_INSUFFICIENT_PRIVILEGE | DatabaseError | 8.4: HY000, 9.7: HY000 | audit log insufficient privilege |
| 3916 | ER_DA_GRP_RPL_STARTED_AUTO_REJOIN | DatabaseError | 8.4: HY000, 9.7: HY000 | da grp rpl started auto rejoin |
| 3917 | ER_SYSVAR_CHANGE_DURING_QUERY | DatabaseError | 8.4: HY000, 9.7: HY000 | sysvar change during query |
| 3918 | ER_GLOBSTAT_CHANGE_DURING_QUERY | DatabaseError | 8.4: HY000, 9.7: HY000 | globstat change during query |
| 3919 | ER_GRP_RPL_MESSAGE_SERVICE_INIT_FAILURE | DatabaseError | 8.4: HY000, 9.7: HY000 | grp rpl message service init failure |
| 3920 | ER_CHANGE_SOURCE_WRONG_COMPRESSION_ALGORITHM_CLIENT | DatabaseError | 8.4: HY000, 9.7: HY000 | change source invalid compression algorithm client |
| 3921 | ER_CHANGE_SOURCE_WRONG_COMPRESSION_LEVEL_CLIENT | DatabaseError | 8.4: HY000, 9.7: HY000 | change source invalid compression level client |
| 3922 | ER_WRONG_COMPRESSION_ALGORITHM_CLIENT | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid compression algorithm client |
| 3923 | ER_WRONG_COMPRESSION_LEVEL_CLIENT | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid compression level client |
| 3924 | ER_CHANGE_SOURCE_WRONG_COMPRESSION_ALGORITHM_LIST_CLIENT | DatabaseError | 8.4: HY000, 9.7: HY000 | change source invalid compression algorithm list client |
| 3925 | ER_CLIENT_PRIVILEGE_CHECKS_USER_CANNOT_BE_ANONYMOUS | DatabaseError | 8.4: HY000, 9.7: HY000 | client privilege checks user cannot be anonymous |
| 3926 | ER_CLIENT_PRIVILEGE_CHECKS_USER_DOES_NOT_EXIST | DatabaseError | 8.4: HY000, 9.7: HY000 | client privilege checks user does not exist |
| 3927 | ER_CLIENT_PRIVILEGE_CHECKS_USER_CORRUPT | DatabaseError | 8.4: HY000, 9.7: HY000 | client privilege checks user corrupt |
| 3928 | ER_CLIENT_PRIVILEGE_CHECKS_USER_NEEDS_RPL_APPLIER_PRIV | DatabaseError | 8.4: HY000, 9.7: HY000 | client privilege checks user needs rpl applier priv |
| 3929 | ER_WARN_DA_PRIVILEGE_NOT_REGISTERED | DatabaseError | 8.4: HY000, 9.7: HY000 | warn da privilege not registered |
| 3930 | ER_CLIENT_KEYRING_UDF_KEY_INVALID | DatabaseError | 8.4: HY000, 9.7: HY000 | client keyring UDF key invalid |
| 3931 | ER_CLIENT_KEYRING_UDF_KEY_TYPE_INVALID | DatabaseError | 8.4: HY000, 9.7: HY000 | client keyring UDF key type invalid |
| 3932 | ER_CLIENT_KEYRING_UDF_KEY_TOO_LONG | DatabaseError | 8.4: HY000, 9.7: HY000 | client keyring UDF key too long |
| 3933 | ER_CLIENT_KEYRING_UDF_KEY_TYPE_TOO_LONG | DatabaseError | 8.4: HY000, 9.7: HY000 | client keyring UDF key type too long |
| 3934 | ER_JSON_SCHEMA_VALIDATION_ERROR_WITH_DETAILED_REPORT | DatabaseError | 8.4: HY000, 9.7: HY000 | JSON schema validation error with detailed report |
| 3935 | ER_DA_UDF_INVALID_CHARSET_SPECIFIED | DatabaseError | 8.4: HY000, 9.7: HY000 | da UDF invalid character set specified |
| 3936 | ER_DA_UDF_INVALID_CHARSET | DatabaseError | 8.4: HY000, 9.7: HY000 | da UDF invalid character set |
| 3937 | ER_DA_UDF_INVALID_COLLATION | DatabaseError | 8.4: HY000, 9.7: HY000 | da UDF invalid collation |
| 3938 | ER_DA_UDF_INVALID_EXTENSION_ARGUMENT_TYPE | DatabaseError | 8.4: HY000, 9.7: HY000 | da UDF invalid extension argument type |
| 3939 | ER_MULTIPLE_CONSTRAINTS_WITH_SAME_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | multiple constraints with same name |
| 3940 | ER_CONSTRAINT_NOT_FOUND | DatabaseError | 8.4: HY000, 9.7: HY000 | constraint not found |
| 3941 | ER_ALTER_CONSTRAINT_ENFORCEMENT_NOT_SUPPORTED | DatabaseError | 8.4: HY000, 9.7: HY000 | alter constraint enforcement not supported |
| 3942 | ER_TABLE_VALUE_CONSTRUCTOR_MUST_HAVE_COLUMNS | DatabaseError | 8.4: HY000, 9.7: HY000 | table value constructor must have columns |
| 3943 | ER_TABLE_VALUE_CONSTRUCTOR_CANNOT_HAVE_DEFAULT | DatabaseError | 8.4: HY000, 9.7: HY000 | table value constructor cannot have default |
| 3944 | ER_CLIENT_QUERY_FAILURE_INVALID_NON_ROW_FORMAT | DatabaseError | 8.4: HY000, 9.7: HY000 | client query failure invalid non row format |
| 3945 | ER_REQUIRE_ROW_FORMAT_INVALID_VALUE | DatabaseError | 8.4: HY000, 9.7: HY000 | require row format invalid value |
| 3946 | ER_FAILED_TO_DETERMINE_IF_ROLE_IS_MANDATORY | DatabaseError | 8.4: HY000, 9.7: HY000 | failed to determine if role is mandatory |
| 3947 | ER_FAILED_TO_FETCH_MANDATORY_ROLE_LIST | DatabaseError | 8.4: HY000, 9.7: HY000 | failed to fetch mandatory role list |
| 3948 | ER_CLIENT_LOCAL_FILES_DISABLED | ProgrammingError | 8.4: 42000, 9.7: 42000 | client local files disabled |
| 3949 | ER_IMP_INCOMPATIBLE_CFG_VERSION | DatabaseError | 8.4: HY000, 9.7: HY000 | imp incompatible cfg version |
| 3950 | ER_DA_OOM | DatabaseError | 8.4: HY000, 9.7: HY000 | da out of memory |
| 3951 | ER_DA_UDF_INVALID_ARGUMENT_TO_SET_CHARSET | DatabaseError | 8.4: HY000, 9.7: HY000 | da UDF invalid argument to set character set |
| 3952 | ER_DA_UDF_INVALID_RETURN_TYPE_TO_SET_CHARSET | DatabaseError | 8.4: HY000, 9.7: HY000 | da UDF invalid return type to set character set |
| 3953 | ER_MULTIPLE_INTO_CLAUSES | DatabaseError | 8.4: HY000, 9.7: HY000 | multiple into clauses |
| 3954 | ER_MISPLACED_INTO | DatabaseError | 8.4: HY000, 9.7: HY000 | misplaced into |
| 3955 | ER_USER_ACCESS_DENIED_FOR_USER_ACCOUNT_BLOCKED_BY_PASSWORD_LOCK | DatabaseError | 8.4: HY000, 9.7: HY000 | user access denied for user account blocked by password lock |
| 3956 | ER_WARN_DEPRECATED_YEAR_UNSIGNED | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated year unsigned |
| 3957 | ER_CLONE_NETWORK_PACKET | DatabaseError | 8.4: HY000, 9.7: HY000 | clone network packet |
| 3958 | ER_SDI_OPERATION_FAILED_MISSING_RECORD | DatabaseError | 8.4: HY000, 9.7: HY000 | sdi operation failed missing record |
| 3959 | ER_DEPENDENT_BY_CHECK_CONSTRAINT | DatabaseError | 8.4: HY000, 9.7: HY000 | dependent by check constraint |
| 3960 | ER_GRP_OPERATION_NOT_ALLOWED_GR_MUST_STOP | DatabaseError | 8.4: HY000, 9.7: HY000 | grp operation not allowed gr must stop |
| 3961 | ER_WARN_DEPRECATED_JSON_TABLE_ON_ERROR_ON_EMPTY | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated JSON table on error on empty |
| 3962 | ER_WARN_DEPRECATED_INNER_INTO | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated inner into |
| 3963 | ER_WARN_DEPRECATED_VALUES_FUNCTION_ALWAYS_NULL | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated values function always null |
| 3964 | ER_WARN_DEPRECATED_SQL_CALC_FOUND_ROWS | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated SQL calc found rows |
| 3965 | ER_WARN_DEPRECATED_FOUND_ROWS | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated found rows |
| 3966 | ER_MISSING_JSON_VALUE | DataError | 8.4: 22035, 9.7: 22035 | missing JSON value |
| 3967 | ER_MULTIPLE_JSON_VALUES | DataError | 8.4: 22034, 9.7: 22034 | multiple JSON values |
| 3968 | ER_HOSTNAME_TOO_LONG | DatabaseError | 8.4: HY000, 9.7: HY000 | hostname too long |
| 3970 | ER_GROUP_REPLICATION_USER_EMPTY_MSG | DatabaseError | 8.4: HY000, 9.7: HY000 | group replication user empty msg |
| 3971 | ER_GROUP_REPLICATION_USER_MANDATORY_MSG | DatabaseError | 8.4: HY000, 9.7: HY000 | group replication user mandatory msg |
| 3972 | ER_GROUP_REPLICATION_PASSWORD_LENGTH | DatabaseError | 8.4: HY000, 9.7: HY000 | group replication password length |
| 3973 | ER_SUBQUERY_TRANSFORM_REJECTED | DatabaseError | 8.4: HY000, 9.7: HY000 | subquery transform rejected |
| 3974 | ER_DA_GRP_RPL_RECOVERY_ENDPOINT_FORMAT | DatabaseError | 8.4: HY000, 9.7: HY000 | da grp rpl recovery endpoint format |
| 3975 | ER_DA_GRP_RPL_RECOVERY_ENDPOINT_INVALID | DatabaseError | 8.4: HY000, 9.7: HY000 | da grp rpl recovery endpoint invalid |
| 3976 | ER_WRONG_VALUE_FOR_VAR_PLUS_ACTIONABLE_PART | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid value for variable plus actionable part |
| 3977 | ER_STATEMENT_NOT_ALLOWED_AFTER_START_TRANSACTION | DatabaseError | 8.4: HY000, 9.7: HY000 | statement not allowed after start transaction |
| 3978 | ER_FOREIGN_KEY_WITH_ATOMIC_CREATE_SELECT | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign key with atomic create select |
| 3979 | ER_NOT_ALLOWED_WITH_START_TRANSACTION | DatabaseError | 8.4: HY000, 9.7: HY000 | not allowed with start transaction |
| 3980 | ER_INVALID_JSON_ATTRIBUTE | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid JSON attribute |
| 3981 | ER_ENGINE_ATTRIBUTE_NOT_SUPPORTED | DatabaseError | 8.4: HY000, 9.7: HY000 | engine attribute not supported |
| 3982 | ER_INVALID_USER_ATTRIBUTE_JSON | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid user attribute JSON |
| 3983 | ER_INNODB_REDO_DISABLED | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb redo disabled |
| 3984 | ER_INNODB_REDO_ARCHIVING_ENABLED | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb redo archiving enabled |
| 3985 | ER_MDL_OUT_OF_RESOURCES | DatabaseError | 8.4: HY000, 9.7: HY000 | mdl out of resources |
| 3986 | ER_IMPLICIT_COMPARISON_FOR_JSON | DatabaseError | 8.4: HY000, 9.7: HY000 | implicit comparison for JSON |
| 3987 | ER_FUNCTION_DOES_NOT_SUPPORT_CHARACTER_SET | DatabaseError | 8.4: HY000, 9.7: HY000 | function does not support character set |
| 3988 | ER_IMPOSSIBLE_STRING_CONVERSION | DatabaseError | 8.4: HY000, 9.7: HY000 | impossible string conversion |
| 3989 | ER_SCHEMA_READ_ONLY | DatabaseError | 8.4: HY000, 9.7: HY000 | schema read only |
| 3990 | ER_RPL_ASYNC_RECONNECT_GTID_MODE_OFF | DatabaseError | 8.4: HY000, 9.7: HY000 | rpl async reconnect GTID mode off |
| 3991 | ER_RPL_ASYNC_RECONNECT_AUTO_POSITION_OFF | DatabaseError | 8.4: HY000, 9.7: HY000 | rpl async reconnect auto position off |
| 3992 | ER_DISABLE_GTID_MODE_REQUIRES_ASYNC_RECONNECT_OFF | DatabaseError | 8.4: HY000, 9.7: HY000 | disable GTID mode requires async reconnect off |
| 3993 | ER_DISABLE_AUTO_POSITION_REQUIRES_ASYNC_RECONNECT_OFF | DatabaseError | 8.4: HY000, 9.7: HY000 | disable auto position requires async reconnect off |
| 3994 | ER_INVALID_PARAMETER_USE | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid parameter use |
| 3995 | ER_CHARACTER_SET_MISMATCH | DatabaseError | 8.4: HY000, 9.7: HY000 | character set mismatch |
| 3996 | ER_WARN_VAR_VALUE_CHANGE_NOT_SUPPORTED | DatabaseError | 8.4: HY000, 9.7: HY000 | warn variable value change not supported |
| 3997 | ER_INVALID_TIME_ZONE_INTERVAL | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid time zone interval |
| 3998 | ER_INVALID_CAST | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid cast |
| 3999 | ER_HYPERGRAPH_NOT_SUPPORTED_YET | ProgrammingError | 8.4: 42000, 9.7: 42000 | hypergraph not supported yet |
| 4000 | ER_WARN_HYPERGRAPH_EXPERIMENTAL | DatabaseError | 8.4: HY000 | warn hypergraph experimental |
| 4001 | ER_DA_NO_ERROR_LOG_PARSER_CONFIGURED | DatabaseError | 8.4: HY000, 9.7: HY000 | da no error log parser configured |
| 4002 | ER_DA_ERROR_LOG_TABLE_DISABLED | DatabaseError | 8.4: HY000, 9.7: HY000 | da error log table disabled |
| 4003 | ER_DA_ERROR_LOG_MULTIPLE_FILTERS | DatabaseError | 8.4: HY000, 9.7: HY000 | da error log multiple filters |
| 4004 | ER_DA_CANT_OPEN_ERROR_LOG | DatabaseError | 8.4: HY000, 9.7: HY000 | da cannot open error log |
| 4005 | ER_USER_REFERENCED_AS_DEFINER | DatabaseError | 8.4: HY000, 9.7: HY000 | user referenced as definer |
| 4006 | ER_CANNOT_USER_REFERENCED_AS_DEFINER | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot user referenced as definer |
| 4007 | ER_REGEX_NUMBER_TOO_BIG | DatabaseError | 8.4: HY000, 9.7: HY000 | regex number too big |
| 4008 | ER_SPVAR_NONINTEGER_TYPE | DatabaseError | 8.4: HY000, 9.7: HY000 | spvar noninteger type |
| 4009 | WARN_UNSUPPORTED_ACL_TABLES_READ | DatabaseError | 8.4: HY000, 9.7: HY000 | unsupported acl tables read |
| 4010 | ER_BINLOG_UNSAFE_ACL_TABLE_READ_IN_DML_DDL | DatabaseError | 8.4: HY000, 9.7: HY000 | binlog unsafe acl table read in DML DDL |
| 4011 | ER_STOP_REPLICA_MONITOR_IO_THREAD_TIMEOUT | DatabaseError | 8.4: HY000, 9.7: HY000 | stop replica monitor I/O thread timeout |
| 4012 | ER_STARTING_REPLICA_MONITOR_IO_THREAD | DatabaseError | 8.4: HY000, 9.7: HY000 | starting replica monitor I/O thread |
| 4013 | ER_CANT_USE_ANONYMOUS_TO_GTID_WITH_GTID_MODE_NOT_ON | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot use anonymous to GTID with GTID mode not on |
| 4014 | ER_CANT_COMBINE_ANONYMOUS_TO_GTID_AND_AUTOPOSITION | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot combine anonymous to GTID and autoposition |
| 4015 | ER_ASSIGN_GTIDS_TO_ANONYMOUS_TRANSACTIONS_REQUIRES_GTID_MODE_ON | DatabaseError | 8.4: HY000, 9.7: HY000 | assign gtids to anonymous transactions requires GTID mode on |
| 4016 | ER_SQL_REPLICA_SKIP_COUNTER_USED_WITH_GTID_MODE_ON | DatabaseError | 8.4: HY000, 9.7: HY000 | SQL replica skip counter used with GTID mode on |
| 4017 | ER_USING_ASSIGN_GTIDS_TO_ANONYMOUS_TRANSACTIONS_AS_LOCAL_OR_UUID | DatabaseError | 8.4: HY000, 9.7: HY000 | using assign gtids to anonymous transactions as local or uuid |
| 4019 | ER_CANT_SET_SQL_AFTER_OR_BEFORE_GTIDS_WITH_ANONYMOUS_TO_GTID | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot set SQL after or before gtids with anonymous to GTID |
| 4020 | ER_ANONYMOUS_TO_GTID_UUID_SAME_AS_GROUP_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | anonymous to GTID uuid same as group name |
| 4021 | ER_CANT_USE_SAME_UUID_AS_GROUP_NAME | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot use same uuid as group name |
| 4022 | ER_GRP_RPL_RECOVERY_CHANNEL_STILL_RUNNING | DatabaseError | 8.4: HY000, 9.7: HY000 | grp rpl recovery channel still running |
| 4023 | ER_INNODB_INVALID_AUTOEXTEND_SIZE_VALUE | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb invalid autoextend size value |
| 4024 | ER_INNODB_INCOMPATIBLE_WITH_TABLESPACE | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb incompatible with tablespace |
| 4025 | ER_INNODB_AUTOEXTEND_SIZE_OUT_OF_RANGE | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb autoextend size out of range |
| 4026 | ER_CANNOT_USE_AUTOEXTEND_SIZE_CLAUSE | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot use autoextend size clause |
| 4027 | ER_ROLE_GRANTED_TO_ITSELF | DatabaseError | 8.4: HY000, 9.7: HY000 | role granted to itself |
| 4028 | ER_TABLE_MUST_HAVE_A_VISIBLE_COLUMN | DatabaseError | 8.4: HY000, 9.7: HY000 | table must have a visible column |
| 4029 | ER_INNODB_COMPRESSION_FAILURE | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb compression failure |
| 4030 | ER_WARN_ASYNC_CONN_FAILOVER_NETWORK_NAMESPACE | DatabaseError | 8.4: HY000, 9.7: HY000 | warn async conn failover network namespace |
| 4031 | ER_CLIENT_INTERACTION_TIMEOUT | DatabaseError | 8.4: HY000, 9.7: HY000 | client interaction timeout |
| 4032 | ER_INVALID_CAST_TO_GEOMETRY | DataError | 8.4: 22S01, 9.7: 22S01 | invalid cast to geometry |
| 4033 | ER_INVALID_CAST_POLYGON_RING_DIRECTION | DataError | 8.4: 22S04, 9.7: 22S04 | invalid cast polygon ring direction |
| 4034 | ER_GIS_DIFFERENT_SRIDS_AGGREGATION | DataError | 8.4: 22S05, 9.7: 22S05 | gis different srids aggregation |
| 4035 | ER_RELOAD_KEYRING_FAILURE | DatabaseError | 8.4: HY000, 9.7: HY000 | reload keyring failure |
| 4036 | ER_SDI_GET_KEYS_INVALID_TABLESPACE | DatabaseError | 8.4: HY000, 9.7: HY000 | sdi get keys invalid tablespace |
| 4037 | ER_CHANGE_RPL_SRC_WRONG_COMPRESSION_ALGORITHM_SIZE | DatabaseError | 8.4: HY000, 9.7: HY000 | change rpl src invalid compression algorithm size |
| 4039 | ER_CANT_USE_SAME_UUID_AS_VIEW_CHANGE_UUID | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot use same uuid as view change uuid |
| 4040 | ER_ANONYMOUS_TO_GTID_UUID_SAME_AS_VIEW_CHANGE_UUID | DatabaseError | 8.4: HY000, 9.7: HY000 | anonymous to GTID uuid same as view change uuid |
| 4041 | ER_GRP_RPL_VIEW_CHANGE_UUID_FAIL_GET_VARIABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | grp rpl view change uuid fail get variable |
| 4042 | ER_WARN_ADUIT_LOG_MAX_SIZE_AND_PRUNE_SECONDS | DatabaseError | 8.4: HY000, 9.7: HY000 | warn aduit log max size and prune seconds |
| 4043 | ER_WARN_ADUIT_LOG_MAX_SIZE_CLOSE_TO_ROTATE_ON_SIZE | DatabaseError | 8.4: HY000, 9.7: HY000 | warn aduit log max size close to rotate on size |
| 4044 | ER_KERBEROS_CREATE_USER | DatabaseError | 8.4: HY000, 9.7: HY000 | kerberos create user |
| 4045 | ER_INSTALL_PLUGIN_CONFLICT_CLIENT | DatabaseError | 8.4: HY000, 9.7: HY000 | install plugin conflict client |
| 4046 | ER_DA_ERROR_LOG_COMPONENT_FLUSH_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | da error log component flush failed |
| 4047 | ER_WARN_SQL_AFTER_MTS_GAPS_GAP_NOT_CALCULATED | DatabaseError | 8.4: HY000, 9.7: HY000 | warn SQL after mts gaps gap not calculated |
| 4048 | ER_INVALID_ASSIGNMENT_TARGET | ProgrammingError | 8.4: 42000, 9.7: 42000 | invalid assignment target |
| 4049 | ER_OPERATION_NOT_ALLOWED_ON_GR_SECONDARY | DatabaseError | 8.4: HY000, 9.7: HY000 | operation not allowed on gr secondary |
| 4050 | ER_GRP_RPL_FAILOVER_CHANNEL_STATUS_PROPAGATION | DatabaseError | 8.4: HY000, 9.7: HY000 | grp rpl failover channel status propagation |
| 4051 | ER_WARN_AUDIT_LOG_FORMAT_UNIX_TIMESTAMP_ONLY_WHEN_JSON | DatabaseError | 8.4: HY000, 9.7: HY000 | warn audit log format unix timestamp only when JSON |
| 4052 | ER_INVALID_MFA_PLUGIN_SPECIFIED | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid mfa plugin specified |
| 4053 | ER_IDENTIFIED_BY_UNSUPPORTED | DatabaseError | 8.4: HY000, 9.7: HY000 | identified by unsupported |
| 4054 | ER_INVALID_PLUGIN_FOR_REGISTRATION | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid plugin for registration |
| 4055 | ER_PLUGIN_REQUIRES_REGISTRATION | DatabaseError | 8.4: HY000, 9.7: HY000 | plugin requires registration |
| 4056 | ER_MFA_METHOD_EXISTS | DatabaseError | 8.4: HY000, 9.7: HY000 | mfa method exists |
| 4057 | ER_MFA_METHOD_NOT_EXISTS | DatabaseError | 8.4: HY000, 9.7: HY000 | mfa method not exists |
| 4058 | ER_AUTHENTICATION_POLICY_MISMATCH | DatabaseError | 8.4: HY000, 9.7: HY000 | authentication policy mismatch |
| 4059 | ER_PLUGIN_REGISTRATION_DONE | DatabaseError | 8.4: HY000, 9.7: HY000 | plugin registration done |
| 4060 | ER_INVALID_USER_FOR_REGISTRATION | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid user for registration |
| 4061 | ER_USER_REGISTRATION_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | user registration failed |
| 4062 | ER_MFA_METHODS_INVALID_ORDER | DatabaseError | 8.4: HY000, 9.7: HY000 | mfa methods invalid order |
| 4063 | ER_MFA_METHODS_IDENTICAL | DatabaseError | 8.4: HY000, 9.7: HY000 | mfa methods identical |
| 4064 | ER_INVALID_MFA_OPERATIONS_FOR_PASSWORDLESS_USER | DatabaseError | 8.4: HY000, 9.7: HY000 | invalid mfa operations for passwordless user |
| 4065 | ER_CHANGE_REPLICATION_SOURCE_NO_OPTIONS_FOR_GTID_ONLY | DatabaseError | 8.4: HY000, 9.7: HY000 | change replication source no options for GTID only |
| 4066 | ER_CHANGE_REP_SOURCE_CANT_DISABLE_REQ_ROW_FORMAT_WITH_GTID_ONLY | DatabaseError | 8.4: HY000, 9.7: HY000 | change rep source cannot disable req row format with GTID only |
| 4067 | ER_CHANGE_REP_SOURCE_CANT_DISABLE_AUTO_POSITION_WITH_GTID_ONLY | DatabaseError | 8.4: HY000, 9.7: HY000 | change rep source cannot disable auto position with GTID only |
| 4068 | ER_CHANGE_REP_SOURCE_CANT_DISABLE_GTID_ONLY_WITHOUT_POSITIONS | DatabaseError | 8.4: HY000, 9.7: HY000 | change rep source cannot disable GTID only without positions |
| 4069 | ER_CHANGE_REP_SOURCE_CANT_DISABLE_AUTO_POS_WITHOUT_POSITIONS | DatabaseError | 8.4: HY000, 9.7: HY000 | change rep source cannot disable auto pos without positions |
| 4070 | ER_CHANGE_REP_SOURCE_GR_CHANNEL_WITH_GTID_MODE_NOT_ON | DatabaseError | 8.4: HY000, 9.7: HY000 | change rep source gr channel with GTID mode not on |
| 4071 | ER_CANT_USE_GTID_ONLY_WITH_GTID_MODE_NOT_ON | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot use GTID only with GTID mode not on |
| 4072 | ER_WARN_C_DISABLE_GTID_ONLY_WITH_SOURCE_AUTO_POS_INVALID_POS | DatabaseError | 8.4: HY000, 9.7: HY000 | warn c disable GTID only with source auto pos invalid pos |
| 4073 | ER_DA_SSL_FIPS_MODE_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | da SSL fips mode error |
| 4074 | ER_VALUE_OUT_OF_RANGE | DatabaseError | 8.4: HY000, 9.7: HY000 | value out of range |
| 4075 | ER_FULLTEXT_WITH_ROLLUP | DatabaseError | 8.4: HY000, 9.7: HY000 | fulltext with rollup |
| 4076 | ER_REGEXP_MISSING_RESOURCE | DatabaseError | 8.4: HY000, 9.7: HY000 | regexp missing resource |
| 4077 | ER_WARN_REGEXP_USING_DEFAULT | DatabaseError | 8.4: HY000, 9.7: HY000 | warn regexp using default |
| 4078 | ER_REGEXP_MISSING_FILE | DatabaseError | 8.4: HY000, 9.7: HY000 | regexp missing file |
| 4079 | ER_WARN_DEPRECATED_COLLATION | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated collation |
| 4080 | ER_CONCURRENT_PROCEDURE_USAGE | DatabaseError | 8.4: HY000, 9.7: HY000 | concurrent procedure usage |
| 4081 | ER_DA_GLOBAL_CONN_LIMIT | DatabaseError | 8.4: HY000, 9.7: HY000 | da global conn limit |
| 4082 | ER_DA_CONN_LIMIT | DatabaseError | 8.4: HY000, 9.7: HY000 | da conn limit |
| 4083 | ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_COLUMN_TYPE_INSTANT | DatabaseError | 8.4: HY000, 9.7: HY000 | alter operation not supported reason column type instant |
| 4084 | ER_WARN_SF_UDF_NAME_COLLISION | DatabaseError | 8.4: HY000, 9.7: HY000 | warn sf UDF name collision |
| 4085 | ER_CANNOT_PURGE_BINLOG_WITH_BACKUP_LOCK | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot purge binlog with backup lock |
| 4086 | ER_TOO_MANY_WINDOWS | DatabaseError | 8.4: HY000, 9.7: HY000 | too many windows |
| 4087 | ER_MYSQLBACKUP_CLIENT_MSG | DatabaseError | 8.4: HY000, 9.7: HY000 | mysqlbackup client msg |
| 4088 | ER_COMMENT_CONTAINS_INVALID_STRING | DatabaseError | 8.4: HY000, 9.7: HY000 | comment contains invalid string |
| 4089 | ER_DEFINITION_CONTAINS_INVALID_STRING | DatabaseError | 8.4: HY000, 9.7: HY000 | definition contains invalid string |
| 4090 | ER_CANT_EXECUTE_COMMAND_WITH_ASSIGNED_GTID_NEXT | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot execute command with assigned GTID next |
| 4091 | ER_XA_TEMP_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | XA temp table |
| 4092 | ER_INNODB_MAX_ROW_VERSION | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb max row version |
| 4094 | ER_OPERATION_NOT_ALLOWED_WHILE_PRIMARY_CHANGE_IS_RUNNING | DatabaseError | 8.4: HY000, 9.7: HY000 | operation not allowed while primary change is running |
| 4095 | ER_WARN_DEPRECATED_DATETIME_DELIMITER | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated datetime delimiter |
| 4096 | ER_WARN_DEPRECATED_SUPERFLUOUS_DELIMITER | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated superfluous delimiter |
| 4097 | ER_CANNOT_PERSIST_SENSITIVE_VARIABLES | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot persist sensitive variables |
| 4098 | ER_WARN_CANNOT_SECURELY_PERSIST_SENSITIVE_VARIABLES | DatabaseError | 8.4: HY000, 9.7: HY000 | warn cannot securely persist sensitive variables |
| 4099 | ER_WARN_TRG_ALREADY_EXISTS | DatabaseError | 8.4: HY000, 9.7: HY000 | warn trg already exists |
| 4100 | ER_IF_NOT_EXISTS_UNSUPPORTED_TRG_EXISTS_ON_DIFFERENT_TABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | if not exists unsupported trg exists on different table |
| 4101 | ER_IF_NOT_EXISTS_UNSUPPORTED_UDF_NATIVE_FCT_NAME_COLLISION | DatabaseError | 8.4: HY000, 9.7: HY000 | if not exists unsupported UDF native fct name collision |
| 4102 | ER_SET_PASSWORD_AUTH_PLUGIN_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | set password auth plugin error |
| 4105 | ER_SRS_INVALID_LATITUDE_OF_ORIGIN | DatabaseError | 8.4: SR002, 9.7: SR002 | spatial reference system invalid latitude of origin |
| 4106 | ER_SRS_INVALID_LONGITUDE_OF_ORIGIN | DatabaseError | 8.4: SR002, 9.7: SR002 | spatial reference system invalid longitude of origin |
| 4107 | ER_SRS_UNUSED_PROJ_PARAMETER_PRESENT | DatabaseError | 8.4: SR002, 9.7: SR002 | spatial reference system unused proj parameter present |
| 4108 | ER_GIPK_COLUMN_EXISTS | DatabaseError | 8.4: HY000, 9.7: HY000 | gipk column exists |
| 4109 | ER_GIPK_FAILED_AUTOINC_COLUMN_EXISTS | DatabaseError | 8.4: HY000, 9.7: HY000 | gipk failed autoinc column exists |
| 4110 | ER_GIPK_COLUMN_ALTER_NOT_ALLOWED | DatabaseError | 8.4: HY000, 9.7: HY000 | gipk column alter not allowed |
| 4111 | ER_DROP_PK_COLUMN_TO_DROP_GIPK | DatabaseError | 8.4: HY000, 9.7: HY000 | drop primary key column to drop gipk |
| 4112 | ER_CREATE_SELECT_WITH_GIPK_DISALLOWED_IN_SBR | DatabaseError | 8.4: HY000, 9.7: HY000 | create select with gipk disallowed in sbr |
| 4114 | ER_CTE_RECURSIVE_NOT_UNION | DatabaseError | 8.4: HY000, 9.7: HY000 | cte recursive not union |
| 4115 | ER_COMMAND_BACKEND_FAILED_TO_FETCH_SECURITY_CTX | DatabaseError | 8.4: HY000, 9.7: HY000 | command backend failed to fetch security ctx |
| 4116 | ER_COMMAND_SERVICE_BACKEND_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | command service backend failed |
| 4117 | ER_CLIENT_FILE_PRIVILEGE_FOR_REPLICATION_CHECKS | DatabaseError | 8.4: HY000, 9.7: HY000 | client file privilege for replication checks |
| 4118 | ER_GROUP_REPLICATION_FORCE_MEMBERS_COMMAND_FAILURE | DatabaseError | 8.4: HY000, 9.7: HY000 | group replication force members command failure |
| 4119 | ER_WARN_DEPRECATED_IDENT | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated ident |
| 4120 | ER_INTERSECT_ALL_MAX_DUPLICATES_EXCEEDED | DatabaseError | 8.4: HY000, 9.7: HY000 | intersect all max duplicates exceeded |
| 4121 | ER_TP_QUERY_THRS_PER_GRP_EXCEEDS_TXN_THR_LIMIT | DatabaseError | 8.4: HY000, 9.7: HY000 | tp query thrs per grp exceeds txn thr limit |
| 4122 | ER_BAD_TIMESTAMP_FORMAT | DatabaseError | 8.4: HY000, 9.7: HY000 | bad timestamp format |
| 4123 | ER_SHAPE_PRIDICTION_UDF | DatabaseError | 8.4: HY000, 9.7: HY000 | shape pridiction UDF |
| 4124 | ER_SRS_INVALID_HEIGHT | DatabaseError | 8.4: SR002, 9.7: SR002 | spatial reference system invalid height |
| 4125 | ER_SRS_INVALID_SCALING | DatabaseError | 8.4: SR002, 9.7: SR002 | spatial reference system invalid scaling |
| 4126 | ER_SRS_INVALID_ZONE_WIDTH | DatabaseError | 8.4: SR002, 9.7: SR002 | spatial reference system invalid zone width |
| 4127 | ER_SRS_INVALID_LATITUDE_POLAR_STERE_VAR_A | DatabaseError | 8.4: SR002, 9.7: SR002 | spatial reference system invalid latitude polar stere variable a |
| 4128 | ER_WARN_DEPRECATED_CLIENT_NO_SCHEMA_OPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated client no schema option |
| 4129 | ER_TABLE_NOT_EMPTY | DatabaseError | 8.4: HY000, 9.7: HY000 | table not empty |
| 4130 | ER_TABLE_NO_PRIMARY_KEY | DatabaseError | 8.4: HY000, 9.7: HY000 | table no primary key |
| 4131 | ER_TABLE_IN_SHARED_TABLESPACE | DatabaseError | 8.4: HY000, 9.7: HY000 | table in shared tablespace |
| 4132 | ER_INDEX_OTHER_THAN_PK | DatabaseError | 8.4: HY000, 9.7: HY000 | index other than primary key |
| 4133 | ER_LOAD_BULK_DATA_UNSORTED | DatabaseError | 8.4: HY000, 9.7: HY000 | load bulk data unsorted |
| 4134 | ER_BULK_EXECUTOR_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk executor error |
| 4135 | ER_BULK_READER_LIBCURL_INIT_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk reader libcurl init failed |
| 4136 | ER_BULK_READER_LIBCURL_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk reader libcurl error |
| 4137 | ER_BULK_READER_SERVER_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk reader server error |
| 4138 | ER_BULK_READER_COMMUNICATION_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk reader communication error |
| 4139 | ER_BULK_LOAD_DATA_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk load data failed |
| 4140 | ER_BULK_LOADER_COLUMN_TOO_BIG_FOR_LEFTOVER_BUFFER | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk loader column too big for leftover buffer |
| 4141 | ER_BULK_LOADER_COMPONENT_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk loader component error |
| 4142 | ER_BULK_LOADER_FILE_CONTAINS_LESS_LINES_THAN_IGNORE_CLAUSE | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk loader file contains less lines than ignore clause |
| 4143 | ER_BULK_PARSER_MISSING_ENCLOSED_BY | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk parser missing enclosed by |
| 4144 | ER_BULK_PARSER_ROW_BUFFER_MAX_TOTAL_COLS_EXCEEDED | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk parser row buffer max total columns exceeded |
| 4145 | ER_BULK_PARSER_COPY_BUFFER_SIZE_EXCEEDED | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk parser copy buffer size exceeded |
| 4146 | ER_BULK_PARSER_UNEXPECTED_END_OF_INPUT | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk parser unexpected end of input |
| 4147 | ER_BULK_PARSER_UNEXPECTED_ROW_TERMINATOR | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk parser unexpected row terminator |
| 4148 | ER_BULK_PARSER_UNEXPECTED_CHAR_AFTER_ENDING_ENCLOSED_BY | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk parser unexpected char after ending enclosed by |
| 4149 | ER_BULK_PARSER_UNEXPECTED_CHAR_AFTER_NULL_ESCAPE | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk parser unexpected char after null escape |
| 4150 | ER_BULK_PARSER_UNEXPECTED_CHAR_AFTER_COLUMN_TERMINATOR | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk parser unexpected char after column terminator |
| 4151 | ER_BULK_PARSER_INCOMPLETE_ESCAPE_SEQUENCE | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk parser incomplete escape sequence |
| 4152 | ER_LOAD_BULK_DATA_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | load bulk data failed |
| 4153 | ER_LOAD_BULK_DATA_WRONG_VALUE_FOR_FIELD | DatabaseError | 8.4: HY000, 9.7: HY000 | load bulk data invalid value for field |
| 4154 | ER_LOAD_BULK_DATA_WARN_NULL_TO_NOTNULL | DatabaseError | 8.4: HY000, 9.7: HY000 | load bulk data warn null to notnull |
| 4155 | ER_REQUIRE_TABLE_PRIMARY_KEY_CHECK_GENERATE_WITH_GR | DatabaseError | 8.4: HY000, 9.7: HY000 | require table primary key check generate with gr |
| 4156 | ER_CANT_CHANGE_SYS_VAR_IN_READ_ONLY_MODE | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot change sys variable in read only mode |
| 4157 | ER_INNODB_INSTANT_ADD_DROP_NOT_SUPPORTED_MAX_SIZE | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb instant add drop not supported max size |
| 4158 | ER_INNODB_INSTANT_ADD_NOT_SUPPORTED_MAX_FIELDS | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb instant add not supported max fields |
| 4159 | ER_CANT_SET_PERSISTED | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot set persisted |
| 4160 | ER_INSTALL_COMPONENT_SET_NULL_VALUE | DatabaseError | 8.4: HY000, 9.7: HY000 | install component set null value |
| 4161 | ER_INSTALL_COMPONENT_SET_UNUSED_VALUE | DatabaseError | 8.4: HY000, 9.7: HY000 | install component set unused value |
| 4162 | ER_WARN_DEPRECATED_USER_DEFINED_COLLATIONS | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated user defined collations |
| 4163 | ER_USER_LOCK_OVERLONG_NAME | ProgrammingError | 8.4: 42000, 9.7: 42000 | user lock overlong name |
| 4164 | ER_WARN_NO_SPACE_VERSION_COMMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | warn no space version comment |
| 4165 | ER_VALIDATE_PASSWORD_INSUFFICIENT_CHANGED_CHARACTERS | DatabaseError | 8.4: HY000, 9.7: HY000 | validate password insufficient changed characters |
| 4166 | ER_WARN_DEPRECATED_WITH_NOTE | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated with note |
| 4167 | ER_CANT_RUN_COMMAND_SERVICES_RECURSIVELY | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot run command services recursively |
| 4168 | ER_WARN_AUDIT_LOG_FILTER_RECOVERY_LOGGING_DISABLED | DatabaseError | 8.4: HY000, 9.7: HY000 | warn audit log filter recovery logging disabled |
| 4169 | ER_CTE_EXPANSION_LIMIT_EXCEEDED | DatabaseError | 8.4: HY000, 9.7: HY000 | cte expansion limit exceeded |
| 6000 | ER_LANGUAGE_COMPONENT | DatabaseError | 8.4: HY000, 9.7: HY000 | language component |
| 6001 | ER_LANGUAGE_COMPONENT_NOT_AVAILABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | language component not available |
| 6002 | ER_LANGUAGE_COMPONENT_UNSUPPORTED_LANGUAGE | DatabaseError | 8.4: HY000, 9.7: HY000 | language component unsupported language |
| 6003 | ER_LANGUAGE_COMPONENT_CANNOT_UNINSTALL | DatabaseError | 8.4: HY000, 9.7: HY000 | language component cannot uninstall |
| 6004 | ER_SP_NO_ALTER_LANGUAGE | DatabaseError | 8.4: HY000, 9.7: HY000 | sp no alter language |
| 6005 | ER_EXPLAIN_INTO_ANALYZE_NOT_SUPPORTED | NotSupportedError | 8.4: 0A000 | explain into analyze not supported |
| 6006 | ER_EXPLAIN_INTO_IMPLICIT_FORMAT_NOT_SUPPORTED | NotSupportedError | 8.4: 0A000, 9.7: 0A000 | explain into implicit format not supported |
| 6007 | ER_EXPLAIN_INTO_FORMAT_NOT_SUPPORTED | NotSupportedError | 8.4: 0A000, 9.7: 0A000 | explain into format not supported |
| 6008 | ER_NULL_CANT_BE_PERSISTED_FOR_READONLY | DatabaseError | 8.4: HY000, 9.7: HY000 | null cannot be persisted for readonly |
| 6009 | ER_EXPLAIN_INTO_FOR_CONNECTION_NOT_SUPPORTED | NotSupportedError | 8.4: 0A000, 9.7: 0A000 | explain into for connection not supported |
| 6010 | ER_INNODB_IMPORT_WRONG_DROPPED_ENUM_LENGTH | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb import invalid dropped enum length |
| 6011 | ER_INNODB_IMPORT_WRONG_NUMBER_OF_INDEXES_ZERO | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb import invalid number of indexes zero |
| 6012 | ER_INNODB_IMPORT_WRONG_NUMBER_OF_INDEXES_TOO_HIGH | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb import invalid number of indexes too high |
| 6013 | ER_INNODB_IMPORT_DROP_COL_METADATA_MISMATCH | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb import drop column metadata mismatch |
| 6014 | ER_INNODB_IMPORT_ENUM_NULL_TERMINATOR_MISSING | DatabaseError | 8.4: HY000, 9.7: HY000 | innodb import enum null terminator missing |
| 6015 | ER_SIMULATED_INJECTION_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | simulated injection error |
| 6016 | ER_WARN_DEPRECATED_DYNAMIC_PRIV_IN_GRANT | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated dynamic priv in grant |
| 6017 | ER_BULK_MULTI_READER_OPEN_FILE_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk multi reader open file failed |
| 6018 | ER_BULK_MULTI_READER_READ_FILE_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk multi reader read file failed |
| 6019 | ER_BULK_MERGE_INVALID_CHUNK | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk merge invalid chunk |
| 6020 | ER_BULK_MERGE_NOT_ALL_CHUNKS_CONSUMED | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk merge not all chunks consumed |
| 6021 | ER_BULK_WRITER_LIBCURL_INIT_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk writer libcurl init failed |
| 6022 | ER_BULK_WRITER_LIBCURL_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk writer libcurl error |
| 6023 | ER_BULK_SORTING_LOADER_WRITE | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk sorting loader write |
| 6024 | ER_BULK_SORTING_LOADER_WAIT | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk sorting loader wait |
| 6025 | ER_BULK_READER_OPEN_FILE_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk reader open file failed |
| 6026 | ER_BULK_LOAD_TABLE_HAS_INSTANT_COLS | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk load table has instant columns |
| 6027 | ER_BULK_LOAD_RESOURCE | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk load resource |
| 6028 | ER_BULK_LOAD_SECONDARY_ENGINE | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk load secondary engine |
| 6029 | ER_BULK_READER_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk reader error |
| 6030 | ER_BULK_READER_FILE_DOESNT_EXIST | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk reader file doesnt exist |
| 6031 | ER_BULK_READER_COULDNT_RESOLVE_HOST | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk reader couldnt resolve host |
| 6032 | ER_START_REPLICA_CHANNEL_INVALID_CONFIGURATION | DatabaseError | 8.4: HY000, 9.7: HY000 | start replica channel invalid configuration |
| 6033 | ER_CANNOT_EXECUTE_IN_PRIMARY | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot execute in primary |
| 6034 | ER_TOO_MANY_GROUP_BY_MODIFIER_BRANCHES | DatabaseError | 8.4: HY000, 9.7: HY000 | too many group by modifier branches |
| 6035 | ER_WARN_DEPRECATED_ENGINE_SYNTAX_NO_REPLACEMENT | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated engine syntax no replacement |
| 6036 | ER_QUALIFY_WITHOUT_WINDOW_FUNCTION | DatabaseError | 8.4: HY000, 9.7: HY000 | qualify without window function |
| 6037 | ER_SUPPORTED_ONLY_WITH_HYPERGRAPH | DatabaseError | 8.4: HY000, 9.7: HY000 | supported only with hypergraph |
| 6038 | ER_SPECIFIC_ACCESS_DENIED | DatabaseError | 8.4: HY000, 9.7: HY000 | specific access denied |
| 6039 | ER_CANT_SET_GTID_NEXT_TO_AUTOMATIC_TAGGED_WHEN_GTID_MODE_IS_OFF | DatabaseError | 8.4: HY000, 9.7: HY000 | cannot set GTID next to automatic tagged when GTID mode is off |
| 6040 | ER_GTID_NEXT_TAG_GTID_MODE_OFF | DatabaseError | 8.4: HY000, 9.7: HY000 | GTID next tag GTID mode off |
| 6041 | ER_LH_COL_NOT_NULLABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse column not nullable |
| 6042 | ER_LH_WARN_COL_MISSING_NOT_NULLABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse warn column missing not nullable |
| 6043 | ER_LH_COL_IS_EMPTY | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse column is empty |
| 6044 | ER_LH_COL_IS_EMPTY_WARN | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse column is empty warn |
| 6045 | ER_LH_BAD_VALUE | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse bad value |
| 6046 | ER_LH_DECIMAL_UNKNOWN_ERR | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse decimal unknown err |
| 6047 | ER_LH_DECIMAL_OOM_ERR | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse decimal out of memory err |
| 6048 | ER_LH_WARN_DECIMAL_ROUNDING | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse warn decimal rounding |
| 6049 | ER_LH_DECIMAL_PRECISION_EXCEEDS_SCHEMA | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse decimal precision exceeds schema |
| 6050 | ER_LH_EXCEEDS_MIN | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse exceeds min |
| 6051 | ER_LH_EXCEEDS_MAX | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse exceeds max |
| 6052 | ER_LH_WARN_EXCEEDS_MIN_TRUNCATING | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse warn exceeds min truncating |
| 6053 | ER_LH_WARN_EXCEEDS_MAX_TRUNCATING | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse warn exceeds max truncating |
| 6054 | ER_LH_REAL_IS_NAN | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse real is nan |
| 6055 | ER_LH_OUT_OF_RANGE | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse out of range |
| 6056 | ER_LH_DATETIME_FORMAT | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse datetime format |
| 6057 | ER_LH_WARN_TRUNCATED | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse warn truncated |
| 6058 | ER_LH_CANNOT_CONVERT_STRING | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse cannot convert string |
| 6059 | ER_LH_RESOURCE_PRINCIPAL_ERR | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse resource principal err |
| 6060 | ER_LH_AWS_AUTH_ERR | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse aws auth err |
| 6061 | ER_LH_CSV_PARSING_ERR | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse csv parsing err |
| 6062 | ER_LH_COLUMN_MISMATCH_ERR | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse column mismatch err |
| 6063 | ER_LH_COLUMN_MAX_ERR | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse column max err |
| 6064 | ER_LH_CHARSET_UNSUPPORTED | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse character set unsupported |
| 6065 | ER_LH_PARQUET_DECIMAL_CONVERSION_ERR | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse parquet decimal conversion err |
| 6066 | ER_LH_STRING_TOO_LONG | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse string too long |
| 6067 | ER_LH_RESOURCE_PRINCIPAL_BUCKET_ERR | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse resource principal bucket err |
| 6068 | ER_LH_NO_FILES_FOUND | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse no files found |
| 6069 | ER_LH_EMPTY_FILE | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse empty file |
| 6070 | ER_LH_DUPLICATE_FILE | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse duplicate file |
| 6071 | ER_LH_AVRO_SCHEMA_DEPTH_EXCEEDS_MAX | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse avro schema depth exceeds max |
| 6072 | ER_LH_AVRO_HEADER_MISMATCH | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse avro header mismatch |
| 6073 | ER_LH_AVRO_ENUM_CANNOT_CONVERT_CHARSET | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse avro enum cannot convert character set |
| 6074 | ER_LH_AVRO_ENUM_MISMATCH | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse avro enum mismatch |
| 6075 | ER_LH_AVRO_TYPE_CANNOT_CONVERT | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse avro type cannot convert |
| 6076 | ER_LH_AVRO_FILE_ENDS_UNEXPECTEDLY | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse avro file ends unexpectedly |
| 6077 | ER_LH_AVRO_FILE_DATA_CORRUPT | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse avro file data corrupt |
| 6078 | ER_LH_AVRO_INVALID_UNION | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse avro invalid union |
| 6079 | ER_LH_AVRO_INVALID_BLOCK_SIZE | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse avro invalid block size |
| 6080 | ER_LH_AVRO_INVALID_BLOCK_RECORD_COUNT | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse avro invalid block record count |
| 6081 | ER_LH_FORMAT_HEADER_NO_MAGIC_BYTES | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse format header no magic bytes |
| 6082 | ER_LH_AVRO_HEADER_METADATA_ERR | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse avro header metadata err |
| 6083 | ER_LH_AVRO_HEADER_NO_SCHEMA | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse avro header no schema |
| 6084 | ER_LH_AVRO_NO_CODEC_IN_HEADER | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse avro no codec in header |
| 6085 | ER_LH_AVRO_INVALID_NAME_IN_SCHEMA | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse avro invalid name in schema |
| 6086 | ER_LH_AVRO_DECODING_ERR | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse avro decoding err |
| 6087 | ER_LH_PARQUET_NON_UTF8_FILE_ENC | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse parquet non utf8 file enc |
| 6088 | ER_LH_PARQUET_SCHEMA_MISMATCH | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse parquet schema mismatch |
| 6089 | ER_LH_PARQUET_ROW_GROUP_SIZE_EXCEEDS_MAX | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse parquet row group size exceeds max |
| 6090 | ER_LH_PARQUET_CANNOT_LOCATE_OFFSET | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse parquet cannot locate offset |
| 6091 | ER_LH_PARQUET_TYPE_CANNOT_CONVERT | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse parquet type cannot convert |
| 6092 | ER_LH_PARQUET_CANNOT_LOCATE_SCHEMA | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse parquet cannot locate schema |
| 6093 | ER_LH_INFER_SCHEMA_MISMATCH / ER_LH_INFER_NUM_COL_MISMATCH | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse infer number column mismatch |
| 6094 | ER_LH_OOM | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse out of memory |
| 6095 | ER_LH_WARN_INFER_SKIPPED_LINES | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse warn infer skipped lines |
| 6096 | ER_LH_WARN_INFER_SKIPPED_FILES | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse warn infer skipped files |
| 6097 | ER_LH_INFER_FILE_HAS_NO_DATA | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse infer file has no data |
| 6098 | ER_LH_INFER_NO_DATA | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse infer no data |
| 6099 | ER_LH_INFER_NO_FILES | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse infer no files |
| 6100 | ER_LH_WARN_INFER_USE_DEFAULT_COL_NAMES | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse warn infer use default column names |
| 6101 | ER_LH_PARQUET_CANNOT_READ_HEADER | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse parquet cannot read header |
| 6102 | ER_LH_INFER_WARN_GOT_EXCEPTION | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse infer warn got exception |
| 6103 | ER_LH_AVRO_CANNOT_PARSE_HEADER | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse avro cannot parse header |
| 6104 | ER_LH_PARQUET_CANT_OPEN_FILE | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse parquet cannot open file |
| 6105 | ER_LH_TOO_LARGE_VALUE_ERR | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse too large value err |
| 6106 | ER_LH_TOO_LARGE_ROW_ERR | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse too large row err |
| 6107 | ER_TABLESAMPLE_PERCENTAGE | DataError | 8.4: 2202H, 9.7: 2202H | tablesample percentage |
| 6108 | ER_TABLESAMPLE_ONLY_ON_BASE_TABLES | DatabaseError | 8.4: HY000, 9.7: HY000 | tablesample only on base tables |
| 6110 | ER_RESULT_SIZE_LIMIT_EXCEEDED | DatabaseError | 8.4: HY000, 9.7: HY000 | result size limit exceeded |
| 6111 | ER_LANGUAGE_COMPONENT_INTERNAL | DatabaseError | 8.4: HY000, 9.7: HY000 | language component internal |
| 6112 | ER_LANGUAGE_COMPONENT_CONCURRENCY_LIMIT | DatabaseError | 8.4: HY000, 9.7: HY000 | language component concurrency limit |
| 6113 | ER_LANGUAGE_COMPONENT_RUNTIME | DatabaseError | 8.4: HY000, 9.7: HY000 | language component runtime |
| 6114 | ER_LANGUAGE_COMPONENT_TIMEZONE | DatabaseError | 8.4: HY000, 9.7: HY000 | language component timezone |
| 6115 | ER_LANGUAGE_COMPONENT_KEYWORD | DatabaseError | 8.4: HY000, 9.7: HY000 | language component keyword |
| 6116 | ER_LANGUAGE_COMPONENT_SET_SYSTEM_VARIABLE | DatabaseError | 8.4: HY000, 9.7: HY000 | language component set system variable |
| 6117 | ER_LANGUAGE_COMPONENT_UNSUPPORTED_TYPE | DatabaseError | 8.4: HY000, 9.7: HY000 | language component unsupported type |
| 6118 | ER_LANGUAGE_COMPONENT_CONVERSION | DatabaseError | 8.4: HY000, 9.7: HY000 | language component conversion |
| 6119 | ER_WARN_SP_STATEMENT_PARTIALLY_EXECUTED | DatabaseError | 8.4: HY000, 9.7: HY000 | warn sp statement partially executed |
| 6120 | ER_STMT_EXECUTION_NOT_ALLOWED_WITHIN_SP_OR_TRG_OR_UDF | DatabaseError | 8.4: HY000, 9.7: HY000 | statement execution not allowed within sp or trg or UDF |
| 6121 | ER_LH_JSON_PARSING | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse JSON parsing |
| 6122 | ER_ENGINE_CANNOT_BE_DEFAULT | DatabaseError | 8.4: HY000, 9.7: HY000 | engine cannot be default |
| 6123 | ER_PARTITION_PREFIX_KEY_NOT_SUPPORTED | DatabaseError | 8.4: HY000, 9.7: HY000 | partition prefix key not supported |
| 6124 | ER_WARN_DEPRECATED_NON_STANDARD_KEY | DatabaseError | 8.4: HY000, 9.7: HY000 | warn deprecated non standard key |
| 6125 | ER_FK_NO_UNIQUE_INDEX_PARENT | DatabaseError | 8.4: HY000, 9.7: HY000 | foreign key no unique index parent |
| 6126 | ER_ACCESS_DENIED_NO_PROXY_GRANT | DatabaseError | 8.4: HY000, 9.7: HY000 | access denied no proxy grant |
| 6127 | ER_ACCESS_DENIED_NO_PROXY | DatabaseError | 8.4: HY000, 9.7: HY000 | access denied no proxy |
| 6128 | ER_LH_USER_DATA_ACCESS_FAILED | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse user data access failed |
| 6129 | ER_BULK_READER_ZSTD_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk reader zstd error |
| 6130 | ER_BULK_PARSER_ERROR | DatabaseError | 8.4: HY000, 9.7: HY000 | bulk parser error |
| 6131 | ER_LH_INVALID_JSON_FILE_FORMAT_SCHEMA | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse invalid JSON file format schema |
| 6132 | ER_LH_INFER_JSON_INVALID_SCHEMA | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse infer JSON invalid schema |
| 6133 | ER_LH_JSON_FILE_FORMAT_WARN_INFER_SCHEMA | DatabaseError | 8.4: HY000, 9.7: HY000 | Lakehouse JSON file format warn infer schema |
| 6134 | ER_NON_SCALAR_USED_AS_KEY | DatabaseError | 8.4: HY000, 9.7: HY000 | non scalar used as key |
| 6135 | ER_INCOMPATIBLE_TYPE_AGG | DatabaseError | 8.4: HY000, 9.7: HY000 | incompatible type agg |
| 6136 | ER_DATA_INCOMPATIBLE_WITH_VECTOR | DatabaseError | 8.4: HY000, 9.7: HY000 | data incompatible with vector |
| 6137 | ER_EXCEEDS_VECTOR_MAX_DIMENSIONS | DatabaseError | 8.4: HY000, 9.7: HY000 | exceeds vector max dimensions |
| 6138 | ER_TO_VECTOR_CONVERSION | DatabaseError | 8.4: HY000, 9.7: HY000 | to vector conversion |
| 6139 | ER_EXTERNAL_UNSUPPORTED_INDEX_ALGORITHM | DatabaseError | 8.4: HY000 | external unsupported index algorithm |
| 6140 | ER_TP_CANNOT_DISABLE_MTL_WITH_DL | DatabaseError | 8.4: HY000, 9.7: HY000 | tp cannot disable mtl with dl |
| 6141 | ER_GRP_RPL_CONNECTION_HANDOFF_BUSY | DatabaseError | 8.4: HY000, 9.7: HY000 | grp rpl connection handoff busy |
| 6400 | ER_SERVER_OFFLINE_MODE_REASON | DatabaseError | 9.7: HY000 | server offline mode reason |
| 6402 | ER_LH_UNSTRUCTURED_NO_DATA_FOUND | DatabaseError | 9.7: HY000 | Lakehouse unstructured no data found |
| 6403 | ER_LH_UNSTRUCTURED_FILE_PARSE_ERR | DatabaseError | 9.7: HY000 | Lakehouse unstructured file parse err |
| 6404 | ER_LH_UNSTRUCTURED_ENCODING_ERR | DatabaseError | 9.7: HY000 | Lakehouse unstructured encoding err |
| 6405 | ER_LH_UNSTRUCTURED_FILE_TYPE_MISMATCH_ERR | DatabaseError | 9.7: HY000 | Lakehouse unstructured file type mismatch err |
| 6406 | ER_LH_UNSTRUCTURED_SCHEMA_ERR | DatabaseError | 9.7: HY000 | Lakehouse unstructured schema err |
| 6407 | ER_LH_COLUMN_ENGINE_ATTR_ERR | DatabaseError | 9.7: HY000 | Lakehouse column engine attr err |
| 6408 | ER_LH_UNSTRUCTURED_OBJ_READ_ERR | DatabaseError | 9.7: HY000 | Lakehouse unstructured obj read err |
| 6409 | ER_LH_NO_SEGMENTS_GENERATED | DatabaseError | 9.7: HY000 | Lakehouse no segments generated |
| 6410 | ER_LH_UNSTRUCTURED_FILE_TOO_LARGE | DatabaseError | 9.7: HY000 | Lakehouse unstructured file too large |
| 6411 | ER_EXCEEDED_MAX_REGULAR_STATEMENT_HANDLE_LIMIT | DatabaseError | 9.7: HY000 | exceeded max regular statement handle limit |
| 6412 | ER_EXPLAIN_ANALYZE_JSON_FORMAT_VERSION_NOT_SUPPORTED | NotSupportedError | 9.7: 0A000 | explain analyze JSON format version not supported |
| 6413 | ER_NON_DML_DYNAMIC_PARAMETERS | DatabaseError | 9.7: HY000 | non DML dynamic parameters |
| 6414 | ER_DEPRECATE_NON_COMPOSABLE_MULTIPLE_ENGINE | DatabaseError | 9.7: HY000 | deprecate non composable multiple engine |
| 6415 | ER_BULK_JSON_INVALID | DatabaseError | 9.7: HY000 | bulk JSON invalid |
| 6416 | ER_BULK_BLOB_TOO_BIG | DatabaseError | 9.7: HY000 | bulk blob too big |
| 6417 | ER_NO_QUERY_PLAN_FOUND | DatabaseError | 9.7: HY000 | no query plan found |
| 6418 | ER_LH_CP_TOO_MANY_CHANGES | DatabaseError | 9.7: HY000 | Lakehouse cp too many changes |
| 6419 | ER_LH_CP_NOT_ENABLED | DatabaseError | 9.7: HY000 | Lakehouse cp not enabled |
| 6420 | ER_LH_CP_NO_VECTOR_STORE | DatabaseError | 9.7: HY000 | Lakehouse cp no vector store |
| 6421 | ER_LH_CP_NO_VALIDATION_MODE | DatabaseError | 9.7: HY000 | Lakehouse cp no validation mode |
| 6423 | ER_LH_CP_COMMIT_FAILED | DatabaseError | 9.7: HY000 | Lakehouse cp commit failed |
| 6424 | ER_LH_UNSTRUCTURED_INVALID_LANGUAGE | DatabaseError | 9.7: HY000 | Lakehouse unstructured invalid language |
| 6425 | ER_LH_VECTOR_TOO_MANY_ENTRIES | DatabaseError | 9.7: HY000 | Lakehouse vector too many entries |
| 6426 | ER_BULK_PARSER_UNEXPECTED_ENCLOSED_BY_CHARACTER | DatabaseError | 9.7: HY000 | bulk parser unexpected enclosed by character |
| 6427 | ER_CMD_NEED_SUPER_OR_CREATE_SPATIAL_REFERENCE_SYSTEM | DatabaseError | 9.7: HY000 | cmd need super or create spatial reference system |
| 6428 | ER_CLAUSE_IGNORED | DatabaseError | 9.7: HY000 | clause ignored |
| 6429 | ER_INVALID_CLAUSE_IN_CONTEXT | DatabaseError | 9.7: HY000 | invalid clause in context |
| 6430 | ER_INVALID_CLAUSE_COMBINATION | DatabaseError | 9.7: HY000 | invalid clause combination |
| 6431 | ER_WARN_REDACTED_PRIVILEGES | DatabaseError | 9.7: HY000 | warn redacted privileges |
| 6432 | ER_INVALID_OUTER_REFERENCE | DatabaseError | 9.7: HY000 | invalid outer reference |
| 6433 | ER_LIBRARIES_NOT_SUPPORTED | DatabaseError | 9.7: HY000 | libraries not supported |
| 6434 | ER_LH_WARN_SUMMARY | DatabaseError | 9.7: HY000 | Lakehouse warn summary |
| 6435 | ER_LH_WARN_INFER_USE_DEFAULT_DELIMITERS | DatabaseError | 9.7: HY000 | Lakehouse warn infer use default delimiters |
| 6436 | ER_LIBRARY_INVALID | DatabaseError | 9.7: HY000 | library invalid |
| 6437 | ER_CANNOT_PERSIST_TRANSACTION_ISOLATION | DatabaseError | 9.7: HY000 | cannot persist transaction isolation |
| 6438 | ER_LH_AVRO_INVALID_RECORD | DatabaseError | 9.7: HY000 | Lakehouse avro invalid record |
| 6439 | ER_LH_INVALID_VALUE_IN_VECTOR | DatabaseError | 9.7: HY000 | Lakehouse invalid value in vector |
| 6440 | ER_LH_WARN_VECTOR_LOSSY_CONVERSION | DatabaseError | 9.7: HY000 | Lakehouse warn vector lossy conversion |
| 6441 | ER_LH_UNSTRUCTURED_TRUNCATION_ERROR | DatabaseError | 9.7: HY000 | Lakehouse unstructured truncation error |
| 6442 | ER_LH_UNSTRUCTURED_PAGE_CHUNK_WARN | DatabaseError | 9.7: HY000 | Lakehouse unstructured page chunk warn |
| 6443 | ER_LH_UNSTRUCTURED_TOO_MANY_CHUNKS | DatabaseError | 9.7: HY000 | Lakehouse unstructured too many chunks |
| 6444 | ER_ACCESS_DENIED_FIREWALL | OperationalError | 9.7: 28000 | access denied firewall |
| 6445 | ER_LH_DECOMPRESSION_ERR | DatabaseError | 9.7: HY000 | Lakehouse decompression err |
| 6446 | ER_JDV_INVALID_DEFINITION_WRONG_ANNOTATIONS | DatabaseError | 9.7: HY000 | jdv invalid definition invalid annotations |
| 6447 | ER_JDV_ALGO_TEMPTABLE_NOT_SUPPORTED | DatabaseError | 9.7: HY000 | jdv algo temptable not supported |
| 6448 | ER_JDV_WITH_CHECK_OPTION_NOT_SUPPORTED | DatabaseError | 9.7: HY000 | jdv with check option not supported |
| 6449 | ER_JDV_INVALID_DEFINITION_NON_SIMPLE_SELECT_NOT_SUPPORTED | DatabaseError | 9.7: HY000 | jdv invalid definition non simple select not supported |
| 6450 | ER_JDV_INVALID_DEFINITION_CTE_NOT_SUPPORTED | DatabaseError | 9.7: HY000 | jdv invalid definition cte not supported |
| 6451 | ER_JDV_INVALID_DEFINITION_GROUPBY_NOT_SUPPORTED | DatabaseError | 9.7: HY000 | jdv invalid definition groupby not supported |
| 6452 | ER_JDV_INVALID_DEFINITION_WINDOW_FUNCTION_NOT_SUPPORTED | DatabaseError | 9.7: HY000 | jdv invalid definition window function not supported |
| 6453 | ER_JDV_INVALID_DEFINITION_LIMIT_NOT_SUPPORTED | DatabaseError | 9.7: HY000 | jdv invalid definition limit not supported |
| 6454 | ER_JDV_INVALID_DEFINITION_ORDERBY_NOT_SUPPORTED | DatabaseError | 9.7: HY000 | jdv invalid definition orderby not supported |
| 6455 | ER_JDV_INVALID_DEFINITION_MULTI_TABLES_NOT_SUPPORTED | DatabaseError | 9.7: HY000 | jdv invalid definition multi tables not supported |
| 6456 | ER_JDV_INVALID_DEFINITION_NO_WHERE_CONDITION_IN_SUBOBJECT | DatabaseError | 9.7: HY000 | jdv invalid definition no where condition in subobject |
| 6457 | ER_JDV_INVALID_DEFINITION_WHERE_CONDITION_IN_ROOTOBJECT | DatabaseError | 9.7: HY000 | jdv invalid definition where condition in rootobject |
| 6458 | ER_JDV_INVALID_DEFINITION_WRONG_WHERE_FORMAT_FOR_SUBOBJECT | DatabaseError | 9.7: HY000 | jdv invalid definition invalid where format for subobject |
| 6459 | ER_JDV_INVALID_DEFINITION_WRONG_FIELD_TYPE | DatabaseError | 9.7: HY000 | jdv invalid definition invalid field type |
| 6460 | ER_JDV_INVALID_DEFINITION_CONTEXT_PREPARE_FAILED | DatabaseError | 9.7: HY000 | jdv invalid definition context prepare failed |
| 6461 | ER_JDV_INVALID_DEFINITION_NO_JSON_OBJECT_IN_ROOT | DatabaseError | 9.7: HY000 | jdv invalid definition no JSON object in root |
| 6462 | ER_JDV_INVALID_DEFINITION_MUSTBE_JSON_OBJECT_FOR_SINGLETON | DatabaseError | 9.7: HY000 | jdv invalid definition mustbe JSON object for singleton |
| 6463 | ER_JDV_INVALID_DEFINITION_MUSTBE_JSON_OBJECT_FOR_NESTED | DatabaseError | 9.7: HY000 | jdv invalid definition mustbe JSON object for nested |
| 6464 | ER_JDV_INVALID_DEFINITION_NO_JSON_OBJ_IN_ARRAYAGG | DatabaseError | 9.7: HY000 | jdv invalid definition no JSON obj in arrayagg |
| 6465 | ER_JDV_INVALID_DEFINITION_RELATIONSHIP_RULES_VIOLATED | DatabaseError | 9.7: HY000 | jdv invalid definition relationship rules violated |
| 6466 | ER_JDV_INVALID_DEFINITION_COLUMN_TYPE_NOT_SUPPORTED | DatabaseError | 9.7: HY000 | jdv invalid definition column type not supported |
| 6467 | ER_JDV_INVALID_DEFINITION_NON_BASE_TABLE_NOT_SUPPORTED | DatabaseError | 9.7: HY000 | jdv invalid definition non base table not supported |
| 6468 | ER_JDV_INVALID_DEFINITION_TABLE_WITHOUT_PK_FOUND | DatabaseError | 9.7: HY000 | jdv invalid definition table without primary key found |
| 6469 | ER_JDV_INVALID_DEFINITION_TABLE_WITHOUT_PK_PROJECTION_FOUND | DatabaseError | 9.7: HY000 | jdv invalid definition table without primary key projection found |
| 6470 | ER_JDV_INVALID_DEFINITION_ID_KEY_USED_BY_NOT_ROOT_TABLE | DatabaseError | 9.7: HY000 | jdv invalid definition id key used by not root table |
| 6471 | ER_JDV_INVALID_DEFINITION_ID_KEY_NOT_USED_BY_ROOT_TABLE | DatabaseError | 9.7: HY000 | jdv invalid definition id key not used by root table |
| 6472 | ER_JDV_INVALID_TABLE_ANNOTATIONS_FOR_SINGLETON_OBJ | DatabaseError | 9.7: HY000 | jdv invalid table annotations for singleton obj |
| 6473 | ER_JDV_INVALID_TABLE_ANNOTATIONS_FOR_NESTED_OBJ | DatabaseError | 9.7: HY000 | jdv invalid table annotations for nested obj |
| 6474 | ER_JDV_INVALID_DEFINITION_DUPLICATE_KEYS_NOT_SUPPORTED | DatabaseError | 9.7: HY000 | jdv invalid definition duplicate keys not supported |
| 6475 | ER_JDV_INVALID_DEFINITION_DUPLICATE_COLUMN_NOT_SUPPORTED | DatabaseError | 9.7: HY000 | jdv invalid definition duplicate column not supported |
| 6476 | ER_JDV_INVALID_DEFINITION_SAME_TABLE_INCONSISTENT_PROJECTION | DatabaseError | 9.7: HY000 | jdv invalid definition same table inconsistent projection |
| 6477 | ER_JDV_INVALID_DEFINITION_COLUMN_LIST_NOT_SUPPORTED | DatabaseError | 9.7: HY000 | jdv invalid definition column list not supported |
| 6478 | ER_JDV_INVALID_DEFINITION_INCONSISTENT_TABLE_FIELD_IN_THE_OBJECT | DatabaseError | 9.7: HY000 | jdv invalid definition inconsistent table field in the object |
| 6479 | ER_JDV_INVALID_DEFINITION_WHERE_USES_NON_IMMEDIATE_PARENT | DatabaseError | 9.7: HY000 | jdv invalid definition where uses non immediate parent |
| 6480 | ER_JDV_INVALID_DEFINITION_COMPOSITE_KEY_USED | DatabaseError | 9.7: HY000 | jdv invalid definition composite key used |
| 6481 | ER_JDV_INVALID_DEFINITION_ALIAS_NOT_USED_FOR_SAME_TABLES | DatabaseError | 9.7: HY000 | jdv invalid definition alias not used for same tables |
| 6482 | ER_JDV_OPERATION_NOT_SUPPORTED | NotSupportedError | 9.7: 0A000 | jdv operation not supported |
| 6483 | ER_JDV_ALTER_OR_REPLACE_NOT_SUPPORTED | DatabaseError | 9.7: HY000 | jdv alter or replace not supported |
| 6484 | ER_JDV_INVALID_JSON_DUALITY_OBJECT_USAGE | DatabaseError | 9.7: HY000 | jdv invalid JSON duality object usage |
| 6485 | ER_JDV_UNEXPECTED_JSON_TYPE | DatabaseError | 9.7: HY000 | jdv unexpected JSON type |
| 6486 | ER_JDV_PRIMARY_KEY_MUST_BE_PROVIDED | DatabaseError | 9.7: HY000 | jdv primary key must be provided |
| 6487 | ER_JDV_JOIN_CONDITION_NOT_SATISFIED | DatabaseError | 9.7: HY000 | jdv join condition not satisfied |
| 6488 | ER_JDV_PK_DUPLICATES_NOT_IDENTICAL | DatabaseError | 9.7: HY000 | jdv primary key duplicates not identical |
| 6489 | ER_JDV_UNMAPPED_KEY | DatabaseError | 9.7: HY000 | jdv unmapped key |
| 6490 | ER_JDV_MISSING_INSERT_TAG | DatabaseError | 9.7: HY000 | jdv missing insert tag |
| 6491 | ER_JDV_MISSING_READONLY_SUBOBJECT | DatabaseError | 9.7: HY000 | jdv missing readonly subobject |
| 6492 | ER_JDV_JOIN_CONDITION_VIOLATION | DatabaseError | 9.7: HY000 | jdv join condition violation |
| 6493 | ER_JDV_MISSING_ETAG | DatabaseError | 9.7: HY000 | jdv missing etag |
| 6494 | ER_JDV_ETAG_MISMATCH | DatabaseError | 9.7: HY000 | jdv etag mismatch |
| 6495 | ER_JDV_MISSING_VALUE | DatabaseError | 9.7: HY000 | jdv missing value |
| 6496 | ER_JDV_PK_UPDATES_NOT_ALLOWED | DatabaseError | 9.7: HY000 | jdv primary key updates not allowed |
| 6497 | ER_JDV_MISSING_UPDATE_TAG | DatabaseError | 9.7: HY000 | jdv missing update tag |
| 6498 | ER_JDV_MISSING_DELETE_TAG | DatabaseError | 9.7: HY000 | jdv missing delete tag |
| 6499 | ER_JDV_NULL_INSERT_VALUE | DatabaseError | 9.7: HY000 | jdv null insert value |
| 6500 | ER_JDV_INSERT_VALUE_NOT_FIXED | DatabaseError | 9.7: HY000 | jdv insert value not fixed |
| 6501 | ER_JDV_NULL_UPDATE_VALUE | DatabaseError | 9.7: HY000 | jdv null update value |
| 6502 | ER_JDV_UPDATE_VALUE_NOT_FIXED | DatabaseError | 9.7: HY000 | jdv update value not fixed |
| 6504 | ER_JDV_CANNOT_BE_USED_WITH | DatabaseError | 9.7: HY000 | jdv cannot be used with |
| 6505 | ER_JDV_DML_INFO | DatabaseError | 9.7: HY000 | jdv DML info |
| 6507 | ER_JDV_INVALID_BINARY_TYPE_HEADER | DatabaseError | 9.7: HY000 | jdv invalid binary type header |
| 6508 | ER_ETAG_NOT_SUPPORTED | DatabaseError | 9.7: HY000 | etag not supported |
| 6509 | ER_ENGINE_ATTRIBUTE_CONFLICT | DatabaseError | 9.7: HY000 | engine attribute conflict |
| 6510 | ER_EXTERNAL_FORMAT_NOT_SUPPORTED | DatabaseError | 9.7: HY000 | external format not supported |
| 6511 | ER_EXTERNAL_TABLE_INVALID_OPTION | DatabaseError | 9.7: HY000 | external table invalid option |
| 6512 | ER_EXTERNAL_TABLE_ENGINE_NOT_SUPPORTED | DatabaseError | 9.7: HY000 | external table engine not supported |
| 6513 | ER_EXTERNAL_TABLE_ENGINE_NOT_SPECIFIED | DatabaseError | 9.7: HY000 | external table engine not specified |
| 6514 | ER_LH_NO_FILES_FOUND_NO_SECURE_FILE_PRIV | DatabaseError | 9.7: HY000 | Lakehouse no files found no secure file priv |
| 6515 | ER_LH_COL_NAME_MISSING | DatabaseError | 9.7: HY000 | Lakehouse column name missing |
| 6516 | ER_LH_COL_NAME_DUPLICATE | DatabaseError | 9.7: HY000 | Lakehouse column name duplicate |
| 6517 | ER_LH_UNSTRUCTURED_OCI_GENAI_FAILURE | DatabaseError | 9.7: HY000 | Lakehouse unstructured oci genai failure |
| 6518 | ER_LH_UNSTRUCTURED_PARTIAL_FILE_PARSE_ERR | DatabaseError | 9.7: HY000 | Lakehouse unstructured partial file parse err |
| 6519 | ER_JDV_EXPLICIT_AUTO_INCREMENT_NOT_ALLOWED | DatabaseError | 9.7: HY000 | jdv explicit auto increment not allowed |
| 6520 | ER_LH_INFER_COL_NAME_MISMATCH | DatabaseError | 9.7: HY000 | Lakehouse infer column name mismatch |
| 6521 | ER_LH_DELTA_SCHEMA_MISMATCH | DatabaseError | 9.7: HY000 | Lakehouse delta schema mismatch |
| 6522 | ER_LH_DELTA_METADATA_READ_ERROR | DatabaseError | 9.7: HY000 | Lakehouse delta metadata read error |
| 6523 | ER_LH_DELTA_UNSUPPORTED_ERROR | DatabaseError | 9.7: HY000 | Lakehouse delta unsupported error |
| 6524 | ER_WARN_ACCOUNT_TRIMMED | DatabaseError | 9.7: HY000 | warn account trimmed |
| 6525 | ER_LH_WARN_DATETIME_MULTIPLE_PARAM_USE | DatabaseError | 9.7: HY000 | Lakehouse warn datetime multiple param use |
| 6526 | ER_LH_GENCOL_ERROR | DatabaseError | 9.7: HY000 | Lakehouse gencol error |
| 6527 | ER_TEMPORAL_FUNCTION_OVERFLOW | DataError | 9.7: 22008 | temporal function overflow |
| 6528 | ER_LH_INFER_COLUMN_MAX_ERR | DatabaseError | 9.7: HY000 | Lakehouse infer column max err |
| 6529 | ER_LH_EXTRA_COLUMNS_IN_FILE | DatabaseError | 9.7: HY000 | Lakehouse extra columns in file |
| 6530 | ER_LH_PARQUET_COMPLEX_CONVERSION | DatabaseError | 9.7: HY000 | Lakehouse parquet complex conversion |
| 6531 | ER_AUDIT_LOG_DISABLED | DatabaseError | 9.7: HY000 | audit log disabled |
| 6532 | ER_AUDIT_LOG_COMPONENT_JSON_FILTER_PARSING_ERROR | DatabaseError | 9.7: HY000 | audit log component JSON filter parsing error |
| 6533 | ER_AUDIT_LOG_FAILED_TO_OPEN_TABLES | DatabaseError | 9.7: HY000 | audit log failed to open tables |
| 6534 | ER_AUDIT_LOG_FAILED_TO_OPEN_TABLE | DatabaseError | 9.7: HY000 | audit log failed to open table |
| 6535 | ER_AUDIT_LOG_COMPONENT_JSON_FILTER_NAME_CANNOT_BE_EMPTY | DatabaseError | 9.7: HY000 | audit log component JSON filter name cannot be empty |
| 6536 | ER_AUDIT_LOG_COMPONENT_USER_NAME_INVALID_CHARACTER | DatabaseError | 9.7: HY000 | audit log component user name invalid character |
| 6537 | ER_AUDIT_LOG_NO_KEYRING_INSTALLED | DatabaseError | 9.7: HY000 | audit log no keyring installed |
| 6538 | ER_AUDIT_LOG_COMPONENT_CANNOT_READ_PASSWORD | DatabaseError | 9.7: HY000 | audit log component cannot read password |
| 6539 | ER_AUDIT_LOG_COMPONENT_HOST_NAME_INVALID_CHARACTER | DatabaseError | 9.7: HY000 | audit log component host name invalid character |
| 6540 | ER_AUDIT_LOG_COMPONENT_JSON_USER_NAME_CANNOT_BE_EMPTY | DatabaseError | 9.7: HY000 | audit log component JSON user name cannot be empty |
| 6541 | ER_AUDIT_LOG_COMPONENT_KEYRING_ID_TIMESTAMP_VALUE_IS_INVALID | DatabaseError | 9.7: HY000 | audit log component keyring id timestamp value is invalid |
| 6542 | ER_AUDIT_LOG_COMPONENT_UDF_INVALID_ARGUMENT_TYPE | DatabaseError | 9.7: HY000 | audit log component UDF invalid argument type |
| 6543 | ER_AUDIT_LOG_COMPONENT_UDF_INVALID_ARGUMENT_COUNT | DatabaseError | 9.7: HY000 | audit log component UDF invalid argument count |
| 6544 | ER_AUDIT_LOG_ABORT | DatabaseError | 9.7: HY000 | audit log abort |
| 6545 | ER_AUDIT_LOG_COULD_NOT_RELOAD_FILTERS | DatabaseError | 9.7: HY000 | audit log could not reload filters |
| 6546 | ER_AUDIT_LOG_UDF_INVALID_ARGUMENT_NULL | DatabaseError | 9.7: HY000 | audit log UDF invalid argument null |
| 6547 | ER_AUDIT_LOG_MAX_SIZE_CLOSE_TO_ROTATE_ON_SIZE | DatabaseError | 9.7: HY000 | audit log max size close to rotate on size |
| 6548 | ER_AUDIT_LOG_MAX_SIZE_AND_PRUNE_SECONDS_PRECEDENCE | DatabaseError | 9.7: HY000 | audit log max size and prune seconds precedence |
| 6549 | ER_AUDIT_LOG_PASSWORD_HAS_NOT_BEEN_SET | DatabaseError | 9.7: HY000 | audit log password has not been set |
| 6550 | ER_AUDIT_LOG_INVALID_TABLE_DEFINITION | DatabaseError | 9.7: HY000 | audit log invalid table definition |
| 6551 | ER_AUDIT_LOG_COMPONENT_FILTER_FAILED_TO_STORE_TABLE_FLDS | DatabaseError | 9.7: HY000 | audit log component filter failed to store table flds |
| 6552 | ER_AUDIT_LOG_COMPONENT_FILTER_FAILED_TO_INIT_TABLE_FOR_READ | DatabaseError | 9.7: HY000 | audit log component filter failed to init table for read |
| 6553 | ER_AUDIT_LOG_COMPONENT_FILTER_FAILED_TO_DELETE_FROM_TABLE | DatabaseError | 9.7: HY000 | audit log component filter failed to delete from table |
| 6554 | ER_AUDIT_LOG_JSON_FILTER_DOES_NOT_EXIST_USER | DatabaseError | 9.7: HY000 | audit log JSON filter does not exist user |
| 6555 | ER_LH_CUSTOM_DATA_PLACEMENT_OVERRIDES | DatabaseError | 9.7: HY000 | Lakehouse custom data placement overrides |
| 6556 | ER_LH_AVRO_FILES_DATA_TYPE_MISMATCH | DatabaseError | 9.7: HY000 | Lakehouse avro files data type mismatch |
| 6557 | ER_AUDIT_LOG_COMPONENT_INVALID_FILTER_ABORT | DatabaseError | 9.7: HY000 | audit log component invalid filter abort |
| 6558 | ER_AUDIT_LOG_COMPONENT_JSON_INVALID_FILTER_CANNOT_BE_USED | DatabaseError | 9.7: HY000 | audit log component JSON invalid filter cannot be used |
| 6559 | ER_WARN_AUDIT_LOG_FILTERS_INVALID | DatabaseError | 9.7: HY000 | warn audit log filters invalid |
| 6560 | ER_JDV_GENERATED_AND_EXPLICIT_PKS_NOT_ALLOWED | DatabaseError | 9.7: HY000 | jdv generated and explicit pks not allowed |
| 6561 | ER_MASKING_POLICY_COMPONENT_ERROR | DatabaseError | 9.7: HY000 | masking policy component error |
| 6562 | ER_MASKING_POLICY_ALREADY_EXISTS | ProgrammingError | 9.7: 42000 | masking policy already exists |
| 6563 | ER_MASKING_POLICY_EXPECTS_CASE | ProgrammingError | 9.7: 42000 | masking policy expects case |
| 6564 | ER_MASKING_POLICY_EXPECTS_CASE_WHEN | ProgrammingError | 9.7: 42000 | masking policy expects case when |
| 6565 | ER_MASKING_POLICY_MISSING_ELSE | ProgrammingError | 9.7: 42000 | masking policy missing else |
| 6566 | ER_MASKING_POLICY_ONE_WHEN_REQUIRED | ProgrammingError | 9.7: 42000 | masking policy one when required |
| 6567 | ER_MASKING_POLICY_MUST_RETURN_ARGUMENT | ProgrammingError | 9.7: 42000 | masking policy must return argument |
| 6568 | ER_MASKING_POLICY_INVALID_GATEKEEPER | ProgrammingError | 9.7: 42000 | masking policy invalid gatekeeper |
| 6569 | ER_MASKING_POLICY_NON_LITERAL_GATEKEEPER_ARG | ProgrammingError | 9.7: 42000 | masking policy non literal gatekeeper argument |
| 6570 | ER_MASKING_POLICY_DISALLOWED_CONSTRUCT | ProgrammingError | 9.7: 42000 | masking policy disallowed construct |
| 6571 | ER_MASKING_POLICY_INVALID_COLUMN_REFERENCE | ProgrammingError | 9.7: 42000 | masking policy invalid column reference |
| 6572 | ER_MASKING_POLICY_INCOMPATIBLE_COLUMN_FEATURE | ProgrammingError | 9.7: 42000 | masking policy incompatible column feature |
| 6573 | ER_MASKING_POLICY_INCOMPATIBLE_TYPES | ProgrammingError | 9.7: 42000 | masking policy incompatible types |
| 6574 | ER_MASKING_POLICY_NON_DETERMINISTIC_FUNC | ProgrammingError | 9.7: 42000 | masking policy non deterministic function |
| 6575 | ER_FK_MAX_TABLES_IN_CASCADE_CHAIN_EXCEEDED | DatabaseError | 9.7: HY000 | foreign key max tables in cascade chain exceeded |
| 6576 | ER_FK_CASCADE_TRIGGER_UPDATING_FK_COLUMNS_NOT_SUPPORTED | DatabaseError | 9.7: HY000 | foreign key cascade trigger updating foreign key columns not supported |
| 6577 | ER_CANT_UPDATE_USED_TABLE_IN_FK_CASCADE | DatabaseError | 9.7: HY000 | cannot update used table in foreign key cascade |
| 6578 | ER_WARN_SET_OPERATIONS_BUFFER | DatabaseError | 9.7: HY000 | warn set operations buffer |
| 6579 | ER_JDV_UNEXPECTED_INVALID_VALUE | DatabaseError | 9.7: HY000 | jdv unexpected invalid value |

## Frozen sources

- {"kind": "official installed client messages", "path": "/opt/homebrew/opt/mysql@8.4/share/mysql/messages_to_clients.txt", "sha256": "8e081588313b9f711f186c16ce21961a182c0dfe343f26c1039a2e5d45e7404b"}
- {"kind": "official installed generated header", "path": "/opt/homebrew/opt/mysql@8.4/include/mysql/mysqld_error.h", "sha256": "bd87e9b79575a335473bed6e782a8b920bb45781145f10fa7b1f4f4822337409"}
- {"kind": "actual dedicated-server registration", "path": "/Users/jef/ResilioSync/code/SQLAero/evidence/rust-sync-kernel/startup/error-catalog-84-observed.json", "sha256": "3fe61eacfc0d2b3af3168bd6da73868c2647297b341399a22a688f6cc83e3470"}
- {"kind": "official shared global error reference", "url": "https://dev.mysql.com/doc/mysql-errors/8.4/en/global-error-reference.html", "path": "/Users/jef/ResilioSync/code/SQLAero/evidence/rust-sync-kernel/startup/global-errors-84.html", "sha256": "88c1c7fbe7181b40152a343773746346b05ecdd94d8a852450465b06ecd20a26"}
- {"kind": "official installed client messages", "path": "/opt/homebrew/opt/mysql@9.7/share/mysql/messages_to_clients.txt", "sha256": "0eeede7d65afc21e57955a530b653a30bfe87ddecf3b36d0f33bf9726cffa211"}
- {"kind": "official installed generated header", "path": "/opt/homebrew/opt/mysql@9.7/include/mysql/mysqld_error.h", "sha256": "2971e130202f870c41fd750bd08960a03f64ccf8d47a4ce06cd789a156ef1b59"}
- {"kind": "actual dedicated-server registration", "path": "/Users/jef/ResilioSync/code/SQLAero/evidence/rust-sync-kernel/startup/error-catalog-97-observed.json", "sha256": "2ffe44137d6e65e735f72c13e94e907048ac2dfca48ad5ef73109448b17f25c2"}
- {"kind": "official shared global error reference", "url": "https://dev.mysql.com/doc/mysql-errors/9.7/en/global-error-reference.html", "path": "/Users/jef/ResilioSync/code/SQLAero/evidence/rust-sync-kernel/startup/global-errors-97.html", "sha256": "59b8731b16d3912f2061a949acae65573d7650f3c131fba6d0f691d93421ec23"}
