interface User {
  id: number;
  name: string;
}

const formatUser = (user: User): string => `${user.id}:${user.name}`;
