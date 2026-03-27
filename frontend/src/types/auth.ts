export type TokenPair = {
  access_token: string;
  refresh_token: string;
  token_type: string;
  expires_in_seconds: number;
};

export type ApiErrorResponse = {
  error?: {
    code?: string;
    message?: string;
  };
};

export type AuthMode = "login" | "signup";

export type StrengthLevel = {
  width: string;
  color: string;
  text: string;
};

export type DocumentRow = {
  id: string;
  title: string;
  file: string;
};

export type SuccessOverlayState = {
  show: boolean;
  title: string;
  sub: string;
};
