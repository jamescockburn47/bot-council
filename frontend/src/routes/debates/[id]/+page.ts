import type { PageLoad } from './$types';

export const load: PageLoad = ({ params }) => ({ debateId: params.id });
