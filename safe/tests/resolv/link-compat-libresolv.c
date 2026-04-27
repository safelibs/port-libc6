extern int dn_skipname (const unsigned char *, const unsigned char *);

static const unsigned char qname[] =
  {
    3, 'w', 'w', 'w',
    7, 'e', 'x', 'a', 'm', 'p', 'l', 'e',
    3, 'c', 'o', 'm',
    0
  };

int
main (void)
{
  return dn_skipname (qname, qname + sizeof (qname)) < 0;
}
