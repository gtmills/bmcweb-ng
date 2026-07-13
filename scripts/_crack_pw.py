#!/usr/bin/env python3
import crypt, warnings
warnings.filterwarnings('ignore')
HASH = '$6$kkSXteT7FmlZdKMQ$e4w/O1sXkPwi9Pzu2ZjKq/l2wZm4JScj7bsVvuPzb6aA6creBixr/7pl0GDWQLpt4nklSNbij8Yttr7esIlfQ0'
candidates = [
    '0penBmc', '0penBMC', 'OpenBmc', 'admin', 'password', '',
    'ibm0penBmc', '0penBmcAdmin', 'admin0penBmc', '0penBmcIBM',
    'ibmadmin', 'ADMIN', 'Admin1234', '0penBmc1', '0penBmc123',
    'admin1234', 'p@ssw0rd', 'Passw0rd', 'ibm1234', 'admin!',
    '0PenBmc', 'open_bmc', 'OpenBMC', 'root', '0penBmcPWD',
]
for pw in candidates:
    try:
        result = crypt.crypt(pw, HASH)
        if result == HASH:
            print(f'MATCH: {pw!r}')
    except Exception as e:
        print(f'Error for {pw!r}: {e}')
print('Done')
